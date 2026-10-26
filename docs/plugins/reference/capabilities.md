# Host capabilities

**Provenance: design.** Generated from the plugin interface's own code, so it cannot drift from it. Do not edit it: `DERETH_TEST_GOLDEN=1 cargo test -p dereth-testkit --test cpu plugins::the_plugin_reference` rewrites it.

Every capability a host may offer. A plugin lists the ones it cannot run without in `capabilities.required` (a host without one refuses to load it) and the ones it uses when they are there in `capabilities.optional`, checking them at run time with `host.has(name)`. Which host offers which is in the [README](../README.md#host-capabilities).

| capability | what it is |
|---|---|
| `core` | Reads, events, chat, actions, the store and tasks. Every host has it. |
| `ui.window` | Declared windows. |
| `ui.hud` | Always-on-screen declared HUDs. |
| `ui.icons` | Icons from the game's data by id. |
| `overlay.world` | Markers and labels on objects or positions in the world. |
| `overlay.arrow` | A direction arrow toward a target. |
| `overlay.radar` | Extra radar blips. |
| `overlay.map` | Points and lines on the world or dungeon map. |
| `map.clicks` | Where the player presses the game's world map, heard as `map.clicked`. |
| `nav.moveto` | The client's own move-to machine. |
| `nav.journey` | Journeys the client plans and carries out: walking round what is in the way, portals and the character's recalls. |
| `nav.path` | The client's walking route to a place or an object, round walls and through                  doors, and whether a straight line between two places is clear: answered                  without moving. |
| `session.pregame` | The character list, and logging in and out. |
| `peers.local` | The bus between the clients on one machine. |
| `net.http` | The plugin's own HTTPS fetches. |
| `messages` | The read-only decoded message stream. |
| `retail_ui` | Moving and hiding the retail interface's panels. |
| `game.data` | The game's own data, read only: landblocks and dungeon cells. |
