# The sample plugins

**Provenance: design.** A guide to the plugins that ship with Dereth, in `dereth/plugins/` of the
repository. [running-the-samples.md](../running-the-samples.md) loads them all at once.

Each page says what the plugin does, its window (which holds everything its commands change; the
commands are shortcuts) and its commands, the permissions its manifest names, and walks through
its source. They are ordered from the simplest to the most involved; each introduces something the
ones before it do not.

| sample | id | shows | names |
|---|---|---|---|
| [XP Meter](xp-meter.md) | `network.dereth.xpmeter` | an event, a window, a command, a timer, the character's store, a module of its own | nothing |
| [Trackers](trackers.md) | `network.dereth.trackers` | Mag-Tools' trackers: combat by creature and element, experience and luminance per hour, worn items' mana, consumables, corpses and players; appraisals a few at a time, objects appearing, moving and going, a window of pages and a HUD, optional capabilities | nothing |
| [Chat Filter](chat-filter.md) | `network.dereth.chatfilter` | rules that hide, highlight and log chat lines, edited in a window; private lines; taking the place of an earlier plugin (`replaces`) | `chat.filter`, `read.chat.private` |
| [ID Notices](id-notices.md) | `network.dereth.idnotices` | appraisals and their promises, properties by number, the spell table, a settings window with pages and a HUD the player moves, toasts | nothing |
| [Looter](looter.md) | `network.dereth.looter` | actions awaited one after another, a loot profile chosen in a window and read back in words, gestures and automation, another plugin's service | `act.interactive`, `auto.loot`, `plugins.call` |
| [Navigator](navigator.md) | `network.dereth.navigator` | GoArrow's ways: one search for places by name and type (a data file, the client's portal knowledge, the world's store, coordinates typed in), Go and Point, the arrow on a place or an object, the character's coordinates, coordinates seen in chat and spots clicked on the map, the route finder's journeys and their alternatives, a dungeon map on a canvas from the game's data, VTank routes | `act.interactive`, `auto.nav` |
| [Auto-login](autologin.md) | `network.dereth.autologin` | the character list, logging in and out, retrying, a window used before the world | `session.control`, `chat.send` |
| [Follow](follow.md) | `network.dereth.follow` | messages between the player's clients, the other clients listed in a window, following | `peers.local`, `act.interactive`, `auto.nav`, `chat.send` |
| [Buffbot](buffbot.md) | `network.dereth.buffbot` | answering tells, a line of players served in turn, casting on others (the `support` domain) and on itself and its gear (`buff`), its own mana and components looked after, a client left running on its own (the headless daemon) | `auto.support`, `auto.buff`, `chat.send`, `read.chat.private` |
| [Inventory](inventory.md) | `network.dereth.inventory` | Mag-Tools' inventory tools: a log of every character's items kept in the world store and searched from any of them, an item's line when it is appraised, stacking, packing by rules, sets of gear, trading by a vendor's profile, and SuitBuilder's best suit from every character's gear; one task doing the work a step at a time, within the time budget, and a search spread over frames | `act.interactive`, `auto.craft`, `auto.vendor` |
| [Tank](tank.md) | `network.dereth.tank` | an unattended macro: a route and its waypoints (jumps, portals, vendors, doors), fighting, looting, vitals, getting unstuck, VTank's settings and profiles, routes made in the game (on a page of its window, or recorded while walking) and saved as VTank files, metas edited in the game, hotkeys and a remote control, services for other plugins | `act.interactive`, `auto.combat`, `auto.nav`, `auto.loot`, `auto.heal`, `auto.buff`, `auto.craft`, `chat.send` |

[parity.md](../../parity.md) sets the samples beside Mag-Tools, VTank, GoArrow and UtilityBelt,
feature by feature.

Every sample is MIT-licensed, as its manifest says, and may be copied as the start of a plugin of
your own; change its `id` first, since two plugins with one id cannot both load.

Releases carry no samples while plugins are experimental; a client built from the repository finds them, each off until the player turns it on ([install-and-update](../install-and-update.md#the-plugins-that-come-with-the-client)).
