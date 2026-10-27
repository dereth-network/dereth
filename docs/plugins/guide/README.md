# Writing and running Dereth plugins: the guide

**Provenance: design.** The plugin interface is this client's own addition, not the original's.
This guide explains it and walks through Dereth's own plugins; the
[specification](../README.md) is the rule, and where the two differ the specification wins.

A **plugin** is a package (a folder, or a zip of one) holding a manifest, `plugin.toml`, and
either Lua scripts or one compiled WebAssembly component. A host (Dereth's desktop client, its
browser client or its headless client) loads it and runs it once per frame
against the game as the client sees it. A plugin can read the game, hear events, print to the chat
window, claim `/commands`, show windows, HUDs and markers, keep settings, and, with the world's
leave, act: loot, cast, move the character, log in.

## Reading order

| if you want to | read |
|---|---|
| write a first plugin in Lua | [getting-started-lua.md](getting-started-lua.md) |
| write one in Rust, compiled to WebAssembly | [getting-started-wasm.md](getting-started-wasm.md) |
| know when a plugin runs, what stops it and what it cannot reach | [lifecycle-and-sandbox.md](lifecycle-and-sandbox.md) |
| install plugins from sources and keep them updated | [install-and-update.md](install-and-update.md) |
| publish your own, in a source of your own | [publishing.md](publishing.md) |
| run the game with the bundled samples and see what each does | [running-the-samples.md](running-the-samples.md) |
| read a working plugin line by line | [samples/](samples/README.md) |
| look up an operation, permission, capability, UI node or limit | [the reference](../reference/README.md) |

## The pieces, and where each is specified

| piece | what it is | the rule |
|---|---|---|
| manifest | `plugin.toml`: id, version, API version, entry file, permissions with reasons, capabilities | [manifest.md](../manifest.md) |
| package | the folder or zip, and a release's hash | [packages.md](../packages.md) |
| plugin source | `plugins.json`: plugins at their latest release, where each zip is and its hash | [sources.md](../sources.md) |
| permissions | what a plugin says it uses; every plugin may, a world's policy bounds automation | [permissions.md](../permissions.md), [policy.md](../policy.md) |
| capabilities | what a host can do; a plugin requires some and checks others at run time | [README](../README.md#host-capabilities) |
| events | what a plugin hears, and in what order within a frame | [events.md](../events.md) |
| the Lua API | the globals a Lua plugin calls | [api.md](../api.md) |
| actions | a plugin asking the game to do something; each answers a promise | [actions.md](../actions.md) |
| windows and HUDs | declared trees of typed nodes, drawn by each host its own way | [ui.md](../ui.md) |
| markers, movement, pre-game, peers, HTTPS | the optional capabilities | [overlays.md](../overlays.md), [nav.md](../nav.md), [pregame.md](../pregame.md), [peers.md](../peers.md) |
| automation | combat, metas, VTank files | [automation.md](../automation.md) |
| compiled plugins | the WebAssembly interface | [wasm.md](../wasm.md) |
| conformance | the test plugins every host is checked against | [conformance.md](../conformance.md) |

## The samples

Eleven sample plugins are in `dereth/plugins/` of the repository. Each has a page in
[samples/](samples/README.md) that says what it does, what it asks for, and walks through its
source.

| sample | what it does |
|---|---|
| [XP Meter](samples/xp-meter.md) | experience this session and per hour, in a window and on `/xp` |
| [Chat Filter](samples/chat-filter.md) | rules that hide, highlight or log chat lines by their words, channel or sender |
| [Looter](samples/looter.md) | loots a corpse or chest by a loot profile |
| [Trackers](samples/trackers.md) | damage dealt and taken, kills, and the corpses around |
| [ID Notices](samples/id-notices.md) | appraises what drops and reports what matters |
| [Navigator](samples/navigator.md) | finds a place as you type, points the way and goes there, through portals and by recalls too |
| [Auto-login](samples/autologin.md) | enters the world as a chosen character |
| [Follow](samples/follow.md) | one client leads, the player's other clients follow |
| [Buffbot](samples/buffbot.md) | buffs players who ask by tell, and keeps its own buffs up |
| [Inventory](samples/inventory.md) | logs every character's items to search them all, with Mag-Tools' inventory tools, and builds the best suit from them |
| [Tank](samples/tank.md) | an unattended macro in the manner of VTank |
