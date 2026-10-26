# Host operations

**Provenance: design.** Generated from the plugin interface's own code, so it cannot drift from it. Do not edit it: `DERETH_TEST_GOLDEN=1 cargo test -p dereth-testkit --test cpu plugins::the_plugin_reference` rewrites it.

Every operation a plugin reaches in its host, from the API's schema (API 0.7.0). A compiled plugin imports each as a function of the WIT interface `host` in [`wit/dereth-plugin.wit`](../wit/dereth-plugin.wit) ([wasm.md](../wasm.md)). A Lua plugin reaches each through the Lua API ([api.md](../api.md)); where one Lua function is the operation one to one, it is named here, and [`dereth.d.luau`](../dereth.d.luau) declares it. The rest of the Lua API (`on`, promises, window and object handles, the libraries) is built on these operations in Lua and is described in [api.md](../api.md).

An operation that fails answers an error saying why (a wrong argument says which). A Lua plugin sees it raised; a compiled one receives it as `err`.

## Types

| schema | WIT | Luau | holds |
|---|---|---|---|
| bool | `bool` | `boolean` | true or false |
| int | `s64` | `number` | a 64-bit signed whole number |
| num | `f64` | `number` | a 64-bit floating-point number |
| str | `string` | `string` | text |
| value | `value` | `any` | any plain value: a table, a list, a string, a number, a boolean or nil; JSON text across the WIT boundary |
| optional | `option<T>` | `T?` | absent (Lua `nil`, WIT `none`), or a value of the inner type |

## By namespace

| namespace | operations |
|---|---|
| `console` | `console.print` |
| `events` | `events.subscribe`, `events.unsubscribe` |
| `chat` | `chat.print`, `chat.hide`, `chat.send` |
| `commands` | `commands.add`, `commands.remove` |
| `task` | `task.timer`, `task.cancel`, `task.error` |
| `store` | `store.get`, `store.set` |
| `player` | `player.get`, `player.int` |
| `object` | `object.exists`, `object.get`, `object.distance`, `object.int`, `object.float`, `object.string`, `object.bool`, `object.int64`, `object.did`, `object.iid`, `object.health`, `object.appraisal`, `object.properties` |
| `inventory` | `inventory.items`, `inventory.open_container`, `inventory.free_slots`, `inventory.packs` |
| `vendor` | `vendor.info` |
| `trade` | `trade.info` |
| `magic` | `magic.spell`, `magic.known` |
| `files` | `files.package`, `files.package_list`, `files.data`, `files.data_list`, `files.save_data` |
| `act` | `act.request` |
| `nav` | `nav.owner`, `nav.journey`, `nav.show_journey`, `nav.known_portals`, `nav.recalls`, `nav.sight`, `nav.parse` |
| `meta` | `meta.parse` |
| `profile` | `profile.usd` |
| `text` | `text.match` |
| `session` | `session.info` |
| `peers` | `peers.send` |
| `overlay` | `overlay.add`, `overlay.remove`, `overlay.clear` |
| `data` | `data.landblock`, `data.cell` |
| `retail_ui` | `retail_ui.set`, `retail_ui.reset` |
| `services` | `services.offer`, `services.withdraw`, `services.reply`, `services.list` |
| `policy` | `policy.info` |
| `loot` | `loot.load`, `loot.rules`, `loot.classify`, `loot.needs_appraisal`, `loot.salvage_combine` |
| `permissions` | `permissions.activation`, `permissions.has` |
| `world` | `world.find`, `world.selection` |
| `ui` | `ui.window`, `ui.hud`, `ui.get`, `ui.set`, `ui.close`, `ui.show`, `ui.toast`, `ui.status` |
| `host` | `host.info`, `host.has` |
| `time` | `time.clock`, `time.unix` |

87 operations in 31 namespaces.

## `console`

### `console.print`

A line for the plugin's own console.

- WIT: `console-print: func(text: string) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `events`

### `events.subscribe`

Hear an event by name from now on (delivered to `deliver`).

- WIT: `events-subscribe: func(event: string) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `events.unsubscribe`

Stop hearing an event.

- WIT: `events-unsubscribe: func(event: string) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `chat`

### `chat.print`

Print a line in the chat window, for the player only; `style` is a chat kind.

- WIT: `chat-print: func(text: string, style: option<string>) -> result<_, string>`
- Lua: `chat.print(text: string, style: string?) -> ()`

### `chat.hide`

Hide an incoming chat line (by its `chat.line` event's `line`); needs `chat.filter`.

- WIT: `chat-hide: func(line: s64) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `chat.send`

Say a line, or type a command, as the player would in the chat bar.

- WIT: `chat-send: func(text: string) -> result<_, string>`
- Lua: `chat.say(text: string) -> ()`

## `commands`

### `commands.add`

Claim a typed `/verb`; the line arrives as a `command` event.

- WIT: `commands-add: func(verb: string, help: string) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `commands.remove`

Give up a claimed `/verb`.

- WIT: `commands-remove: func(verb: string) -> result<_, string>`
- Lua: `commands.remove(verb: string) -> ()`

## `task`

### `task.timer`

Start a timer; each time it is due a `task.timer` event with its id arrives.

- WIT: `task-timer: func(ms: f64, repeat: bool) -> result<s64, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `task.cancel`

Cancel a timer.

- WIT: `task-cancel: func(timer: s64) -> result<_, string>`
- Lua: `task.cancel(timer: number) -> ()`

### `task.error`

Report an error the plugin caught, to its console.

- WIT: `task-error: func(message: string) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `store`

### `store.get`

A value kept in a store scope (`global`, `world` or `character`).

- WIT: `store-get: func(scope: string, key: string) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `store.set`

Keep a value in a store scope; `null` removes it.

- WIT: `store-set: func(scope: string, key: string, data: value) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `player`

### `player.get`

A field of the player's character (`name`, `health`, `xp`, `position`, ...).

- WIT: `player-get: func(field: string) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `player.int`

The character's own integer property `key`, or null.

- WIT: `player-int: func(key: s64) -> result<value, string>`
- Lua: `game.player_int(key: number) -> any`

## `object`

### `object.exists`

Whether the client knows the object.

- WIT: `object-exists: func(object: s64) -> result<bool, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.get`

A field of an object (`name`, `class`, `container`, `position`, ...).

- WIT: `object-get: func(object: s64, field: string) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.distance`

The object's distance from the character in metres, or null.

- WIT: `object-distance: func(object: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.int`

An integer property of an object, or null.

- WIT: `object-int: func(object: s64, key: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.float`

A float property of an object, or null.

- WIT: `object-float: func(object: s64, key: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.string`

A string property of an object, or null.

- WIT: `object-string: func(object: s64, key: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.bool`

A boolean property of an object, or null.

- WIT: `object-bool: func(object: s64, key: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.int64`

A 64-bit integer property of an object, or null.

- WIT: `object-int64: func(object: s64, key: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.did`

A data-id property of an object (a number naming game data), or null.

- WIT: `object-did: func(object: s64, key: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.iid`

An instance-id property of an object (another object's id), or null.

- WIT: `object-iid: func(object: s64, key: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.health`

What the client knows of an object's health (`current`, `max`, `fraction`), or null.

- WIT: `object-health: func(object: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.appraisal`

The last appraisal of an object, or null.

- WIT: `object-appraisal: func(object: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `object.properties`

Every property the client knows of an object (`ints`, `int64s`, `floats`, `strings`, `bools`, `dids`, `iids`, each by number), or one type's, or null.

- WIT: `object-properties: func(object: s64, type: option<string>) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `inventory`

### `inventory.items`

The ids of the items the character holds.

- WIT: `inventory-items: func() -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `inventory.open_container`

The container the character has open, or null.

- WIT: `inventory-open-container: func() -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `inventory.free_slots`

Free slots in the main pack, or null.

- WIT: `inventory-free-slots: func() -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `inventory.packs`

The character's packs in the order the main pack shows them, the main pack first: `id`, `name`, `capacity`, `used` each, or null.

- WIT: `inventory-packs: func() -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `vendor`

### `vendor.info`

The open vendor, or null.

- WIT: `vendor-info: func() -> result<value, string>`
- Lua: `vendor.info() -> any`

## `trade`

### `trade.info`

The open trade, or null.

- WIT: `trade-info: func() -> result<value, string>`
- Lua: `trade.info() -> any`

## `magic`

### `magic.spell`

A spell of the spell table, or null.

- WIT: `magic-spell: func(spell: s64) -> result<value, string>`
- Lua: `magic.spell(spell: number) -> any`

### `magic.known`

The character's spell book, in id order: every spell it knows that the spell table has, or `count` of them from the `from`th (from 1).

- WIT: `magic-known: func(from: option<s64>, count: option<s64>) -> result<value, string>`
- Lua: `magic.known(from: number?, count: number?) -> any`

## `files`

### `files.package`

A text file of the plugin's own package.

- WIT: `files-package: func(path: string) -> result<option<string>, string>`
- Lua: `files.package(path: string) -> string?`

### `files.package_list`

Every file of the plugin's own package, by path.

- WIT: `files-package-list: func() -> result<value, string>`
- Lua: `files.package_list() -> any`

### `files.data`

A text file of the plugin's data folder (the player's files for it), or null.

- WIT: `files-data: func(path: string) -> result<option<string>, string>`
- Lua: `files.data(path: string) -> string?`

### `files.data_list`

Every file in the plugin's data folder, by path.

- WIT: `files-data-list: func() -> result<value, string>`
- Lua: `files.data_list() -> any`

### `files.save_data`

Save a text file to the plugin's data folder (at most 4 MiB, 1000 files).

- WIT: `files-save-data: func(path: string, text: string) -> result<bool, string>`
- Lua: `files.save_data(path: string, text: string) -> boolean`

## `act`

### `act.request`

Ask for an action (`act.use`, `combat.attack`, `nav.move_to`, `net.fetch`, ...); answers its id, and its end arrives as `action.completed` or `action.failed`.

- WIT: `act-request: func(action: string, args: value) -> result<option<s64>, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `nav`

### `nav.owner`

The plugin moving the character, or none.

- WIT: `nav-owner: func() -> result<option<string>, string>`
- Lua: `nav.owner() -> string?`

### `nav.journey`

The journey this plugin has under way (its legs, the leg being carried out, where that leg goes, the way still to go, the goal), or null.

- WIT: `nav-journey: func() -> result<value, string>`
- Lua: `nav.journey() -> any`

### `nav.show_journey`

Show or hide the host's drawing of this plugin's journey (the arrow, the way on the map and radar); false when it has none.

- WIT: `nav-show-journey: func(on: bool) -> result<bool, string>`
- Lua: `nav.show_journey(on: boolean) -> boolean`

### `nav.known_portals`

The portals this world's store knows: where each stands and sends, how it is known (learned, published, hint), its uses, failures and level range, and why it was retired.

- WIT: `nav-known-portals: func() -> result<value, string>`
- Lua: `nav.known_portals() -> any`

### `nav.recalls`

Where the character's recalls have sent it: each recall (`spell <id>`, `lifestone`, `marketplace`) and its landing.

- WIT: `nav-recalls: func() -> result<value, string>`
- Lua: `nav.recalls() -> any`

### `nav.sight`

Whether a straight line from the character (or `from`) to an object or a place is clear of the world's walls, ground and the objects that stop a body, a metre above each end; null where the host cannot tell.

- WIT: `nav-sight: func(target: value, from: option<value>) -> result<option<bool>, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `nav.parse`

A VTank route file's text, read.

- WIT: `nav-parse: func(text: string) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `meta`

### `meta.parse`

A VTank meta file's text, read.

- WIT: `meta-parse: func(text: string) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `profile`

### `profile.usd`

A VTank settings profile's text, read: each table by name, as rows of columns.

- WIT: `profile-usd: func(text: string) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `text`

### `text.match`

The first match of a pattern in a text, with its groups, or null.

- WIT: `text-match: func(pattern: string, text: string, ignore-case: bool) -> result<value, string>`
- Lua: `regex.match(pattern: string, text: string, ignore_case: boolean) -> any`

## `session`

### `session.info`

The session: its phase, the character list, portal space.

- WIT: `session-info: func() -> result<value, string>`
- Lua: `session.info() -> any`

## `peers`

### `peers.send`

Send a message to this plugin's copies in the player's other clients.

- WIT: `peers-send: func(channel: string, data: value) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `overlay`

### `overlay.add`

Add a marker (`world`, `radar`, `map` or `arrow`); answers its id.

- WIT: `overlay-add: func(kind: string, marker: value) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `overlay.remove`

Remove a marker.

- WIT: `overlay-remove: func(marker: s64) -> result<bool, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `overlay.clear`

Remove every marker of a kind, or all.

- WIT: `overlay-clear: func(kind: option<string>) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `data`

### `data.landblock`

A landblock of the game's data (`0xXXYY`, or a cell within it): its 9 by 9 heights, terrain and roads, and its cells, or null.

- WIT: `data-landblock: func(landblock: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `data.cell`

A dungeon or building cell of the game's data: its place, its portals and the cells seen from it, its floor's outline and its box, or null.

- WIT: `data-cell: func(cell: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `retail_ui`

### `retail_ui.set`

Hide or show (`hidden`) and move (`x`, `y`) one of the game's own panels.

- WIT: `retail-ui-set: func(panel: string, state: value) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `retail_ui.reset`

Give one of the game's own panels back as the player had it.

- WIT: `retail-ui-reset: func(panel: string) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `services`

### `services.offer`

Offer a service to the other plugins in this client (only those `allow` lists, a list of plugin ids, when given); its calls arrive as `service.call` events.

- WIT: `services-offer: func(service: string, allow: option<value>) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `services.withdraw`

Stop offering a service.

- WIT: `services-withdraw: func(service: string) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `services.reply`

Answer a `service.call`: its value, or the error it failed with.

- WIT: `services-reply: func(call: s64, data: value, error: option<string>) -> result<bool, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `services.list`

The services this plugin may call: `{ plugin, service }` each.

- WIT: `services-list: func() -> result<value, string>`
- Lua: `services.list() -> any`

## `policy`

### `policy.info`

The world's plugin policy.

- WIT: `policy-info: func() -> result<value, string>`
- Lua: `policy.info() -> any`

## `loot`

### `loot.load`

Read a VTank loot profile; answers its handle.

- WIT: `loot-load: func(text: string) -> result<s64, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `loot.rules`

A loaded loot profile's rules.

- WIT: `loot-rules: func(profile: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `loot.classify`

The rule an item meets first, or null.

- WIT: `loot-classify: func(profile: s64, object: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `loot.needs_appraisal`

Whether an item must be appraised before it can be classified.

- WIT: `loot-needs-appraisal: func(profile: s64, object: s64) -> result<bool, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `loot.salvage_combine`

A loaded loot profile's salvage combining: the workmanship ranges by material, or null.

- WIT: `loot-salvage-combine: func(profile: s64) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `permissions`

### `permissions.activation`

Seconds left of the activation the player's last gesture gave.

- WIT: `permissions-activation: func() -> result<f64, string>`
- Lua: `permissions.activation() -> number`

### `permissions.has`

Whether the plugin may use what a permission covers: every one but net.http, which its manifest must name.

- WIT: `permissions-has: func(permission: string) -> result<bool, string>`
- Lua: `permissions.has(permission: string) -> boolean`

## `world`

### `world.find`

The ids of the objects a filter matches (by name, class, type, container, wielder, distance and property conditions).

- WIT: `world-find: func(filter: value) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `world.selection`

The selected object, or null.

- WIT: `world-selection: func() -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `ui`

### `ui.window`

Declare (or replace) a window from its UI tree; a new one opens only while the plugin answers the player.

- WIT: `ui-window: func(tree: value) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `ui.hud`

Declare (or replace) a HUD from its UI tree; a new one opens only while the plugin answers the player.

- WIT: `ui-hud: func(tree: value) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `ui.get`

A property of a node of a declared window.

- WIT: `ui-get: func(window: string, node: string, property: string) -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `ui.set`

Set a property of a node of a declared window.

- WIT: `ui-set: func(window: string, node: string, property: string, data: value) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `ui.close`

Close a declared window.

- WIT: `ui-close: func(window: string) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `ui.show`

Show a declared window again, while the plugin answers the player; otherwise it stays closed.

- WIT: `ui-show: func(window: string) -> result<_, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `ui.toast`

Show a line briefly at the top of the screen with the plugin's name (1 to 30 seconds, 5 when left out; three at once a plugin).

- WIT: `ui-toast: func(text: string, seconds: option<f64>) -> result<_, string>`
- Lua: `ui.toast(text: string, seconds: number?) -> ()`

### `ui.status`

What the plugin bar shows the plugin doing: running, paused or idle.

- WIT: `ui-status: func(status: string) -> result<_, string>`
- Lua: `ui.status(status: string) -> ()`

## `host`

### `host.info`

The host's name and API version.

- WIT: `host-info: func() -> result<value, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `host.has`

Whether the host offers a capability.

- WIT: `host-has: func(capability: string) -> result<bool, string>`
- Lua: through the Lua API ([api.md](../api.md))

## `time`

### `time.clock`

Seconds since the plugin loaded, on the frame clock.

- WIT: `time-clock: func() -> result<f64, string>`
- Lua: through the Lua API ([api.md](../api.md))

### `time.unix`

The game's time, as Unix seconds.

- WIT: `time-unix: func() -> result<s64, string>`
- Lua: through the Lua API ([api.md](../api.md))
