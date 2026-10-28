# Dereth plugins: the specification (draft 0.7)

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A **plugin** is a package of Lua scripts (or a compiled WebAssembly component, [wasm.md](wasm.md)) that a client loads to add to the game: a meter, a log, a
timer, a window of its own. This document and its companions are the plugin interface: what a
package holds, what a plugin may do, what it hears, what it reads, what it can show, and how a
host is tested against all of it.

The interface belongs to no one client. Dereth's desktop client, its headless client and its web
client implement it, and so can any other host: a different client, or an add-on that runs Dereth
plugins inside another program. A plugin written against this document runs in every host that
passes the conformance profiles it needs.

**This is draft 0.7.** The API version is `0.7`; a plugin written for `0.1` to `0.6` runs
unchanged. Until 1.0 a minor version may still change things; after it, minor versions only add.

**Plugins are experimental.** A plugin loads only while it is ticked in the plugin panel
(`/plugin`): the ones the player installs or adds through the launcher start ticked, and the
others wait for the player. Its windows and HUDs open only when the player asks
([ui.md](ui.md#opening-a-window)).

To write a plugin, start with [the guide](guide/README.md): getting started in Lua and in
WebAssembly, a plugin's lifecycle and sandbox, installing and updating, publishing, and each
sample walked through. To look something up, [the reference](reference/README.md) lists every operation,
permission, capability, UI node, limit and conformance case, generated from the code.

| document | what it covers |
|---|---|
| [manifest.md](manifest.md) | `plugin.toml`: a package's identity, entry file, permissions and capabilities |
| [packages.md](packages.md) | folders and zips, installing one from a source, the `dereth-plugin` tool |
| [sources.md](sources.md) | plugin sources: `plugins.json`, a list of plugins at their latest release, and how a release is checked |
| [permissions.md](permissions.md) | what a plugin may do, a file the player picks, gestures and automation |
| [events.md](events.md) | what a plugin hears, and in what order |
| [api.md](api.md) | the Lua functions: reading the game, chat, commands, tasks, the stores, actions, loot profiles |
| [actions.md](actions.md) | every action: what it sends, what ends it, its timeout and its domain |
| [ui.md](ui.md) | declared windows and HUDs: the node types, their properties and their events |
| [overlays.md](overlays.md) | markers in the world, on the radar and the map, the arrow, shapes and floating text in the world, and nametags |
| [messages.md](messages.md) | the read-only message stream: each message received or sent, decoded |
| [services.md](services.md) | services one plugin offers the others in the same client |
| [nav.md](nav.md) | places, moving the character, and routes (`dereth.nav`, VTank `.nav` files) |
| [pregame.md](pregame.md) | the character list, entering the world, logging off, and why an entry failed |
| [peers.md](peers.md) | the player's other clients on the machine, and HTTPS to listed domains |
| [policy.md](policy.md) | a world's plugin policy |
| [automation.md](automation.md) | combat actions, the player-killer and parity rules, and metas (`dereth.meta`) |
| [audit.md](audit.md) | the answer to a server that asks which plugins are running |
| [ubscript.md](ubscript.md) | scripts written for UtilityBelt's scripting, run as they are: its script API, HUDs and ImGui, and what it answers |
| [conformance.md](conformance.md) | the conformance suite: test plugins, profiles and the host driver |
| [wasm.md](wasm.md) | compiled plugins: WebAssembly components against [`wit/dereth-plugin.wit`](wit/dereth-plugin.wit), generated with [`dereth.d.luau`](dereth.d.luau) from the API's schema |
| [guide/](guide/README.md) | the guide: getting started in Lua and WebAssembly, lifecycle and sandbox, install and update, publishing, running the samples, each sample walked through |
| [reference/](reference/README.md) | the reference, generated from the code: operations, permissions, capabilities, UI nodes, limits, conformance cases |

## A plugin in one page

```lua
-- main.lua
local earned = 0
local win = ui.window{ id = "main", title = "XP",
  ui.label{ id = "earned", text = "0" },
  ui.button{ id = "reset", text = "Reset" },
}
on("character.xp", function(e)
  earned = earned + e.gained
  win:get("earned").text = tostring(earned)
end)
on("ui.clicked", function(e)
  if e.id == "reset" then earned = 0; win:get("earned").text = "0" end
end)
commands.add("xp", "shows the experience earned", function()
  chat.print("Earned " .. earned .. " this session.")
end)
```

```toml
# plugin.toml
id = "example.xp"
name = "XP"
version = "1.0.0"
api = "0.1"
```

Eleven sample plugins are in `dereth/plugins/` of the repository, each walked through in
[the guide](guide/samples/README.md): an XP meter, a chat logger and filter, a looter driven by a
loot profile, combat and corpse trackers, ID notices that appraise what drops, a navigator that
points the way and walks there, auto-login, multi-client follow, a buffbot, an inventory log and
tools in the manner of Mag-Tools, and an unattended macro in the manner of VTank (the Tank), as a
reference for automation. [parity.md](parity.md) sets them beside the plugins players know,
feature by feature.

## How a plugin runs

**Lifecycle.** A host finds packages, checks each manifest (its API version against the host's,
its required host capabilities against what the host offers), makes a fresh sandboxed VM for it and runs its entry file. The entry file's top level registers handlers and
declares windows. A plugin is unloaded by running its global `on_unload` function (if it defines
one), writing its store, and dropping its windows, timers, commands and VM. A reload unloads and
loads again; what `on_unload` returned is handed to the new instance's `on_reload`.

**Where it runs.** A plugin is active only in the parts of the session its manifest names in
`contexts`: `world` (the default), `pregame` (the login and character screens) and `chargen`
(character creation). Outside them it hears only the session's comings and goings, its timers
wait, its windows are hidden, and the host lets go of the character for it; it hears
`context.begin` and `context.end` as it becomes active and stops being so. See
[events](events.md#where-a-plugin-runs).

**One point in the frame.** Plugins run on the client's own thread, once per frame, after the
frame's messages from the server have been applied and before the interface's own frame. A plugin
always sees a game state consistent with the events it is handed, and what it changes in its
windows is drawn in the same frame.

**Order.** Events are delivered in the order the client applied the messages behind them, so a
replayed recording yields the same events in the same order. This is what makes the conformance
suite possible.

**Faults.** A failing handler is told in the chat, its first line under the plugin's name, and
written whole, with its traceback, to the client's log; the other handlers of the same event still
run. Each entry into a plugin (loading it, or delivering one
event) has a time budget, 4 ms by default, and a VM has a memory cap, 64 MiB by default. The
time is the entry's, the host's answers to its calls included (a search, a pattern matched, a file
written). A handler past its budget is stopped (a Lua script is told where it was, and has a
millisecond to handle the error; a caught overrun still counts). Loading a plugin (its entry
file's top level) has twenty-five entries' budgets, and so has the entry telling it it is active
(`context.begin`), where it sets itself up for that part of the session; neither is held to the
plugin's share of the frame, and compiling its own source is not counted.
A plugin's entries in one frame together have a share of the frame, two entries' budgets, and all
plugins' together three: past either, the events still due for a plugin wait, in order, for the
next frame. Ten failures within a minute, three entries stopped by the budget within a minute, or
thirty frames past its share within a minute, suspend the plugin and the player is told. A value a plugin hands the host is copied
within bounds (at most 262,144 values and 16 MiB, a table that appears twice counted twice, and
one that holds itself refused), and the host keeps at most 200 lines and 256 KiB of what a plugin
printed, each line cut at 4 KiB.

**The sandbox.** A VM has Lua's safe libraries (`string`, `table`, `math`, `utf8`, `bit32`,
`coroutine`, and `os.clock`, `os.time`, `os.date`, `os.difftime` on the host's clock). It has no
file, process or network functions and cannot compile code except through `require`, which loads
only modules from the plugin's own package. The libraries and the plugin interface are read-only;
a plugin's own globals live in a table of its own. Plugins never share a VM; one reaches another
only through a [service](services.md) it offers.

**The language.** The language is Luau, a Lua 5.1 derivative with optional type annotations. A
plugin that must also run in a host with plain Lua 5.1 keeps to the portable subset: no type
annotations, no compound assignment (`+=`), no string interpolation, no `continue`, no `//`.

## Host capabilities

A host says what it can do; a plugin lists what it needs (`capabilities.required`, and the load is
refused without them) and checks the rest at run time with `host.has(name)`.

| capability | what it is |
|---|---|
| `core` | reading the game, events, chat, commands, tasks, the store: every host |
| `ui.window` | declared windows (a host with no display keeps the tree for tests) |
| `ui.hud` | always-on-screen declared windows with no frame |
| `ui.icons` | icons from the game's data |
| `overlay.world`, `overlay.arrow`, `overlay.radar`, `overlay.map` | markers, shapes, text and nametags in the world, a direction arrow, radar blips, map marks |
| `map.clicks` | where the player presses the game's world map, heard as `map.clicked` ([events](events.md#clicks-on-the-map)) |
| `nav.moveto` | moving the character (turning and the run lock) |
| `nav.journey` | journeys the client plans and carries out: walking, portals and recalls ([nav](nav.md#journeys)) |
| `nav.path` | the client's walking route to a target and whether a straight line to it is clear, answered without moving ([nav](nav.md#paths-and-lines-of-sight)) |
| `session.pregame` | the character list, logging in and out |
| `peers.local` | the bus between clients on one machine |
| `net.http` | the plugin's own HTTPS fetches |
| `messages` | a read-only stream of decoded messages |
| `retail_ui` | moving and hiding the game's own panels |
| `game.data` | the game's own data, read only: landblocks and dungeon cells |

Draft 0.7 defines every capability.

| host | `host.name` | capabilities (0.7) |
|---|---|---|
| Dereth, desktop | `dereth-desktop` | `core`, `ui.window`, `ui.hud`, `ui.icons`, the four `overlay.*`, `map.clicks`, `nav.moveto`, `nav.journey`, `nav.path`, `session.pregame`, `peers.local`, `net.http`, `messages`, `retail_ui`, `game.data` |
| Dereth in a browser | `dereth-web` | as the desktop (`peers.local` and `net.http` where the page offers them) |
| Dereth, headless | `dereth-headless` | `core`, `ui.window`, `ui.hud` (the trees are kept and can be clicked by a script; nothing is drawn), `session.pregame`, `peers.local` (a bus inside its process, or with `--peers <dir>` the other headless clients naming that folder), `messages`, and `nav.moveto`, `nav.journey`, `nav.path` and `game.data` with `--world` |
| the retail client, through Decal | `dereth-decal` | `core`, `ui.window` and `ui.hud` (as Decal views), `overlay.world` and `overlay.arrow` (as Decal's 3-D objects), `nav.moveto`, `session.pregame` (the list and logging off; a plugin cannot choose the character), `messages`, `peers.local` (with the desktop clients on the same machine) and `game.data` (the game's own files); an action Decal has no call for fails as `unavailable`, and an attack, a jump and a held movement key are the game's own keys, pressed only with the player's leave and while the game's window is in front ([the Decal host](../../dereth/decal/README.md)) |

**What each interface draws.** The capabilities a host offers do not change with the interface the
player chooses, and every interface of the desktop and browser clients draws everything: windows,
HUDs, toasts, the plugin bar, the host's own windows, labels, nametags, floating text, the arrow,
world markers and shapes, and marks on the radar and the map where the interface has them; and
each interface's world map tells the plugins where the player presses it. In every one the player
unlocks the HUDs from the plugin panel to drag any plugin's HUD where they want it
([ui.md](ui.md#huds)).

| interface | plugin windows | the host's dialog for a file and its own windows |
|---|---|---|
| modern | in the game's own window art (its panels' frame and tab, its buttons, check boxes, scrollbars, sliders, fields and dividers), font, icons and colours, standing among the interface's own windows (a press brings one forward) | as the plugin windows, over them |
| classic | in the classic interface's own art (its pages' ground and title strip, its buttons, check boxes, scrollbar, choices and dividers), text and colours, over the interface's windows and under its own message box | as the plugin windows, over them |
| Horizon | as Horizon's own windows, in its art: framed, stacked and closed like them, the pad's focus, cancel, tabs and on-screen keyboard working on them, an entry each on the pad's menu; HUDs placed in its HUD layout editor, or dragged while unlocked; commands on its cross hotbars | as Horizon's own windows and dialogs |

Where a part of the game's art is missing from the data (an older or a trimmed set of files), that
part is drawn plainly in the interface's colours and the rest from the art. Under the modern and
the classic interface, which take no pad of their own, the pad drives the plugins' surfaces: the
d-pad moves a focus among their controls, A presses, B closes the window or refuses the host's
dialog, the
shoulder buttons turn tabs and View moves from window to window and to the plugin bar. The focus
is the player's: a window a plugin opens of its own accord never takes it.

A file a plugin asks for is the host's dialog under every interface
([permissions](permissions.md#a-file-the-player-picks)): drawn as a dialog of that interface,
with the plugin windows still drawn beside it. Nothing else is asked: a plugin the player turns
on loads at once.

**Where Dereth finds plugins.** The desktop and browser clients load the plugins that are on in
the plugin panel (`/plugin`). The desktop client finds every package in the `plugins` folder of
its settings folder and beside the program, each off until the player turns it on, and in each
folder named with `--offer-plugins <dir>` (the launcher's installs and the player's folders it
lists), each on the first time the client finds it; a folder named with `--plugins <dir>` loads at
once. `--no-plugins` starts it with none, `--plugin-dev` reloads a plugin whenever one of its files
changes, and `--plugin-policy` names the world's [policy](policy.md) (the launcher passes it).
Stores are kept in the `plugin-store` folder beside it. The headless client loads the folders
named with `--plugins` and keeps stores where `--plugin-store` says, or in memory. The browser
client finds the plugin folders and zips the player added on its start page (*Add a plugin
folder…*), and the plugins installed there from plugin sources, which it keeps in the browser's
private storage with the stores. Players install plugins from plugin sources
([sources.md](sources.md)) through the launcher, which keeps them for the desktop client, and on
the browser client's start page.

Scripts written for UtilityBelt's scripting run as they are, a script's folder dropped in the
plugins folder: [ubscript.md](ubscript.md).

Parts of the plugin interface are ported from MIT-licensed plugins for the retail client; their
notices are in [THIRD-PARTY.md](THIRD-PARTY.md).
