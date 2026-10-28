# A plugin's life, and its sandbox

**Provenance: design.** A walk-through of the plugin interface; the rules are in the
[specification](../README.md#how-a-plugin-runs), and every number here is in the generated
[limits](../reference/limits.md).

In short: a host loads a plugin once it has checked the manifest; runs its entry file; then, once a frame, hands it the frame's events. A plugin that
keeps failing, or keeps running past its time budget, is suspended. Unloading runs the plugin's
`on_unload`, writes its stores and takes everything it made down with it. Inside, a plugin reaches
nothing but its own package and the host.

## Loading

Plugins are experimental. A host loads plugins when it starts: the desktop and browser clients
the plugins that are on in the plugin panel (the launcher's start on, the others when the player
turns them on), and the desktop client each `--plugins` folder; the headless client each
`--plugins` folder ([install-and-update.md](install-and-update.md)). For each package, in order,
the host:

1. **reads the manifest**, and refuses a package whose manifest does not read (an unknown key, a
   malformed id or version, an entry file that is not there);
2. **checks the API version**: it runs a plugin when the majors are equal and its own minor is at
   least the plugin's (`it needs plugin API 0.6, and this host has 0.5`);
3. **checks the required capabilities** against what it offers (`this host (dereth-headless) has
   no nav.moveto`), and the manifest's `hosts.exclude`;
4. **refuses a second plugin with the same id**;
5. **picks the engine** by the entry file (`.lua` or `.luau` the Lua engine, `.wasm` the
   WebAssembly one), makes a fresh VM for the plugin alone, and **runs the entry file**. The entry
   file's top level (or a compiled plugin's `init`) registers handlers, claims commands, declares
   windows and starts timers. Its windows start closed: the player opens them
   ([ui.md](../ui.md#opening-a-window)).

A refusal at any step is told to the player in the chat window (`example.kills was not loaded:
…`) and written to the host's log. An entry file that raises an error leaves the plugin
**faulted**: what it had set up before the error (windows, timers, commands) goes with it, and the
chat says why (`example.kills failed to load: …`, the error's first line; the whole of it, its
traceback too, is in the client's log).

After a successful load a plugin that listens for `plugin.loaded` hears it, and the chat says so
by the plugin's name (`[Plugins] Kill Counter loaded.`; `enabled.` when the player's Enable loaded
it). The stores open as they become available: `store.global` at once, `store.world` and
`store.character` at login.

## Running: one point in the frame

Plugins run on the client's own thread, once per frame, after the frame's messages from the
server have been applied and before the interface draws. In each frame the host:

1. carries out the loads, unloads and reloads asked for since the last frame (and, with
   `--plugin-dev`, reloads a plugin whose files changed, checked once a second);
2. turns the frame's state into events, in the fixed order of [events.md](../events.md#order);
3. delivers each event to every running plugin that listens for it, in load order, and within a
   plugin to its handlers in the order they were registered;
4. takes back what the plugins asked for: lines to print and to hide, commands claimed, and the
   next queued action's requests, which go through the same interaction step as the player's own
   clicks.

A plugin therefore always reads a game consistent with the events it is handed, and a replayed
recording gives the same events in the same order, which is what makes the
[conformance suite](../conformance.md) possible. Timers and `task.sleep` run on the frame clock,
so a replay fires them on the same frames too.

## Where it runs: contexts

A plugin is loaded when the client starts, usually at the character screen, but it is only
**active** in the parts of the session its manifest names:

```toml
contexts = ["world"]              # the default: only while a character is in the world
# contexts = ["pregame", "world"] # an auto-login: the character screen, and the world after
# contexts = ["chargen"]          # something for the character creation screens
```

Outside its contexts its windows and HUDs are hidden, its timers wait, its commands are not
run, and it hears only the lifecycle events: `session.pregame`, `session.login`,
`session.logout` and the like, and `context.begin` / `context.end` as it becomes active and
stops being so. When it stops being active the host stops what it was doing with the character
for it (its actions fail `stopped`, its movement ends), so a hunting plugin cannot go on
fighting at the character screen. Its windows come back as they were when it is active again.

```lua
on("context.begin", function(e) print("active in", e.context) end)
on("context.end", function(e) print("idle until the world comes back") end)
```

## Faults and suspension

Each entry into a plugin (loading it, or delivering one event) has a **time budget**, 4 ms by
default, and each VM a **memory cap**, 64 MiB by default. A Lua VM checks the clock at every loop
and function call and stops an entry past its deadline; a compiled plugin counts its work instead
(100,000 units per millisecond of the budget), so it stops at the same point on every host.
Either way the budget is the plugin's own work: time the host spends answering its calls (a
printed line written to the log, a long list built) is not counted.
Allocating past the cap fails as an out-of-memory error.

In Lua the overrun is an error naming where the script was (`the plugin ran past its time budget
(at engine:93 in note < engine:1086)`). The script has a further millisecond to handle it: a task
whose loop wraps its step in `pcall` notes the error and sleeps, and lives on. Past that the entry
is stopped whatever it catches, and an overrun the script caught still counts as one. Work too
long for one entry (reading a spell book of thousands, a long file) belongs in a task that
`task.sleep(0)`s between pieces.

To see where a Lua plugin's time goes, start the desktop client with `DERETH_PLUGIN_PROFILE=1`.
Every half minute each plugin writes to the client's log (`dereth::plugin` at debug) its entries
by event (count, total, longest), the host calls it made by operation, and the time its own Lua
spent by function and by line.

- A handler that raises an error is told in the chat, its first line under the plugin's name, and
  written whole, with its traceback, to the client's log; the other handlers of the same event
  still run.
- **Ten errors within a minute**, or **three entries stopped by the budget within a minute**,
  **suspend** the plugin. The player is told (`example.kills was suspended (10 errors within a
  minute). /plugin reload example.kills starts it again.`). A suspended plugin keeps its VM and
  its windows but hears nothing until it is reloaded. Since it can no longer let go of anything
  itself, the host lets go for it at once, as at unloading: its queued actions are dropped, its
  movement stops and the run lock it set is let go, the attack it started is cancelled while the
  client is still attacking, and its markers, pre-game operation and HTTPS requests go.

A plugin's state is one of `running`, `faulted` (its entry failed; it holds no VM) or `suspended`,
shown with the reason by `/plugin list` and the plugin panel.

## Unloading and reloading

`/plugin disable <id>` (or the panel's Disable) unloads a plugin and keeps it off at the next
start; `/plugin reload <id>` (or Reload) unloads it and loads it again; the client's shutdown
unloads every plugin. The host:

1. runs the plugin's global `on_unload()`, if it defines one (a compiled plugin receives the event
   `plugin.unload`), and keeps what it returns;
2. drops the plugin's queued actions, stops its movement and lets go of the run lock it set,
   cancels the attack it started if the client is still attacking, drops its pending pre-game
   operation and its HTTPS requests, and takes down its markers;
3. writes its stores;
4. releases its commands, and drops its windows, timers and VM.

`/plugin reload <id>` unloads the plugin and loads it again from its source, so edited files are
read afresh; what `on_unload` returned is handed to the new instance's `on_reload(state)` (a
compiled plugin receives `plugin.reload` with it). The value must be plain: nil, a boolean, a
number, a string or a table of them.

```lua
local count = 0
function on_unload()
	return { count = count }
end
function on_reload(state)
	count = state.count
end
```

Stores are written at logout, at unload and every 60 seconds, so a store survives a crash with at
most a minute lost; locals survive only a reload, through `on_unload`.

## The player's controls

`/plugin` opens the plugin panel: every plugin with its state, why the chosen one stopped, and
four buttons: **Enable** (lit for a plugin that is not running), **Disable** (lit for one that is
enabled), **Reload** (lit for one running) and **Reload All** (lit while any runs). In the chat
bar:

| command | does |
|---|---|
| `/plugin list` | each plugin's id, version and state |
| `/plugin enable <id>`, `/plugin disable <id>` | as the panel's buttons; a plugin named to load is not disabled |
| `/plugin reload <id>` | as the panel's button; it also loads a plugin named to load that is not loaded |
| `/plugin policy` | the world's policy in force |
| `/plugin huds unlock`, `/plugin huds lock` | let the player drag any plugin's HUD to where they want it, kept for the character; lock them again |

What a plugin prints, and each error it meets, is told in the chat under its name; the chat also
says when a plugin is loaded, reloaded, unloaded, enabled or disabled
([api.md](../api.md#commands)). Logging out closes every plugin window and HUD, and the panel.

The headless client's script has the same verbs as `plugin load <folder or zip>`, `plugin unload`,
`plugin reload`, `plugin list` and `plugin console`.

## The sandbox

A plugin reaches the game only through the host's operations ([reference](../reference/operations.md)),
each checked against the world's policy and the host's limits, and nothing else:

- **No files, processes or network.** A Lua VM has Luau's safe libraries (`string`, `table`,
  `math`, `utf8`, `bit32`, `coroutine`, and `os.clock`, `os.time`, `os.date`, `os.difftime` on the
  host's clock); a compiled plugin may import only the interface's own functions, and one that
  imports anything else is refused at load. The only file a plugin reads is one of its own
  package's (`files.package`, `require`); the only network is HTTPS to the domains its manifest
  lists, with the `net.http` permission named.
- **No code but its own.** Source is compiled only through `require`, only from the plugin's own
  package, and only as source text; `loadstring` is gone. A package holds no native code: only the
  file types of [limits](../reference/limits.md#packages).
- **Nothing shared.** Every plugin has a VM of its own. The libraries and the plugin interface are
  read-only; a plugin's own globals live in a table of its own. Plugins never see each other's
  state; the same plugin in the player's other clients is reachable only through `peers`.
- **Only what the interface would send.** An action becomes the requests the game's own interface
  makes for the same gesture, through the client's own checks; a plugin has no way to write to the
  network itself. Actions wait in one queue for all plugins, one outstanding at a time
  ([actions.md](../actions.md#the-queue)).
- **Rate ceilings** hold for every plugin: one pending interactive request, five appraisals a
  second, chat sent at most once a second with a burst of three, fifty local lines a frame
  ([permissions.md](../permissions.md#rate-ceilings)).
- **Bounded output.** A window holds at most 2,000 nodes, 16 deep; a store 16 MiB; a plugin at most
  256 markers and 256 queued actions; the host keeps the last 200 lines it printed.
