# Events

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A plugin hears events by name: `on(name, handler)` registers a handler, `off(name, handler)`
removes it. An event is named `domain.event` and its handler receives one table. Subscribing to a
name this version does not define is an error, so a misspelt name is caught at once.

## Order

Once per frame, at the host's one point in the frame, the host delivers the frame's events in this
order:

0. **Contexts**: `context.end` to each plugin no longer active, then `context.begin` to each
   plugin now active, in load order (see [Where a plugin runs](#where-a-plugin-runs)); then
   **messages**: to a plugin listening, each of the frame's messages as `messages.<name>`
   and `messages.any` ([the message stream](messages.md)), before the changes they made.
1. **Session**: `session.login_failed` (an entry the server refused), then `session.pregame` (the
   character list is up) or `session.disconnect`; then `session.logout`, then `session.login`,
   then `session.enter_world`; then `session.portal_enter` or `session.portal_exit`.
2. **Objects**: every `world.object_created`, then every `world.object_removed`, each in id order;
   then every `world.object_changed`, then every `world.object_moved`, each in id order.
3. **Selection**: `world.selection_changed`.
4. **Character**: `character.vital` (health, stamina, mana), `character.xp`,
   `character.luminance`, `character.death`, `character.fellowship`, then
   `character.enchantment_added` and `character.enchantment_removed`.
5. **Inventory**: `inventory.container_closed` then `inventory.container_opened`; the player's
   items added, moved and removed, each in id order; then `items.appraised`, in id order.
6. **Commerce**: `commerce.vendor_closed` then `commerce.vendor_opened`; then the trade's
   `commerce.trade_opened`, `commerce.trade_changed` or `commerce.trade_closed`.
7. **Actions**: a journey's `nav.journey_planned`, `nav.journey_leg` and `nav.journey_learned`,
   in the order they happened, to the plugin whose journey it is; then `action.completed` and
   `action.failed`, each to the plugin whose action it was, after the state changes that settled
   it.
8. **Chat**: each `chat.line` in the order the lines arrived.
9. **Commands** the player typed, each to the plugin that claimed it.
10. **Policy**: `session.world_policy`, when the policy in force changed.
11. **Peers**: each `peers.message`, in the order it arrived, to the plugin of the id that sent it.
12. **Window input**: the `ui.*` events and the `map.clicked` events since the last frame, in the
    order the player acted.
13. **Timers** that came due, earliest first.

Within one event, handlers run in the order they were registered, and plugins in the order they
were loaded. The first frame after login sets the baseline: the values a character logs in with
are not changes.

## The events of 0.7

| event | the table's fields |
|---|---|
| `session.pregame` | `characters` (a list of `{ id, name }`), `world`, `account`: the character list is up, after connecting and after each log-off |
| `session.login` | `id` (the player's object id), `character` (the character's name) |
| `session.login_failed` | `code`, `reason`: the server refused an entry (see [pre-game](pregame.md)) |
| `session.enter_world` | `id` |
| `session.portal_enter` | none: the character entered portal space (a teleport, or the arrival in the world at login) |
| `session.portal_exit` | none: the character has been placed at the teleport's destination |
| `session.logout` | none |
| `session.disconnect` | none: the connection ended |
| `session.world_policy` | `published`, `rules` (each domain and `session` to its rule) |
| `world.object_created` | `id`, `name`, `class` (see [the Lua interface](api.md#objects)) |
| `world.object_changed` | `id`, `changed`: the keys whose value differs from the last frame's, in this order: `name`, `stack`, `value`, `burden`, `container`, `wielder`, `icon`, `slot`, `uses`, `flags` (the public description's flags), `appraisal` (a new appraisal arrived), `health` (a fellow's figures or a creature's appraisal). An object's first frame sets its baseline |
| `world.object_moved` | `id`, `position` (a place table), `moved`: metres from where it was last said to be. Said when an object with a place in the world is a metre or more from where the last one put it; its first place sets the baseline, and an object taken into a container is forgotten |
| `world.object_removed` | `id` |
| `world.selection_changed` | `id` (nil when nothing is selected), `previous` |
| `character.vital` | `vital` (`health`, `stamina` or `mana`), `current`, `max`, `previous` |
| `character.xp` | `total`, `unassigned`, `gained` (the change in `total`; 0 when only `unassigned` moved) |
| `character.luminance` | `earned`, `max`, `gained` |
| `character.death` | `deaths`: the character died (its count of deaths rose) |
| `character.enchantment_added` | `spell`, `name`, `remaining` (seconds; nil when it does not run out) |
| `character.enchantment_removed` | `spell`, `name` |
| `character.fellowship` | `change`: `joined`, `left`, or `changed` (a fellow came or went, or the leader or the name changed); `name`, `members` (their ids). A fellow's vitals are `world.object_changed` with `health`, and `game.player.fellowship` |
| `inventory.container_opened` | `id`: the corpse, chest or other container whose contents have arrived |
| `inventory.container_closed` | `id` |
| `inventory.item_added` | `id`, `container` (the player's id for the main pack and for what is worn) |
| `inventory.item_removed` | `id` |
| `inventory.item_moved` | `id`, `container`, `previous` |
| `items.appraised` | `id`: an appraisal arrived, whoever asked for it |
| `commerce.vendor_opened` | `id` (the vendor) |
| `commerce.vendor_closed` | `id` |
| `commerce.trade_opened` | `partner` |
| `commerce.trade_changed` | `partner`: an item was added or taken back, or either side accepted |
| `commerce.trade_closed` | none |
| `chat.line` | `line` (its place among this frame's lines, for `chat.hide`), `text`, `kind`, `type` (the line's type number, which `kind` names), `window`, `prefix` |
| `ui.clicked` | `window`, `id` |
| `ui.changed` | `window`, `id`, `value` |
| `ui.selected` | `window`, `id`, `rows` (the selected rows, from 1) |
| `ui.closed` | `window`: the player closed it, from its close box or the plugin bar |
| `ui.opened` | `window`: the player opened it from the plugin bar |
| `ui.resized` | `window`, `width`, `height`: the player let go of a resizable window's edge, corner or grip at this size, in character cells and lines, within the bounds the window declares |
| `map.clicked` | `position` (`{ ns, ew }`: the place under the pointer, to a thousandth), `button` (`left`, `right` or `middle`), `modifiers` (`{ ctrl, shift, alt }`, each true while held): the player pressed the game's world map ([below](#clicks-on-the-map)) |
| `action.completed` | `id`, `action` (its name, `items.move`), `value` (an appraisal's table; nil for most) |
| `service.call` | `id`, `from` (the calling plugin), `service`, `args`, `gesture`: a call to a service this plugin offers ([services.md](services.md)); the Lua interface answers it with the offered function |
| `action.failed` | `id`, `action`, `reason` (see [actions](actions.md#how-an-action-ends)) |
| `nav.journey_planned` | `id` (the journey's promise), `legs`, `seconds`, `reason` (why it was planned again, or nil): a journey of this plugin's was planned ([nav](nav.md#journeys)) |
| `nav.journey_leg` | `id`, `index` (from 1), `kind` (`walk`, `door`, `portal`, `recall` or `hand_back`), `to`, and by kind `path`, `object`, `name`, `class`, `from`, `source`, `recall`, `item`, `token`: a leg of the journey began |
| `nav.journey_learned` | `id`, `kind` (`portal`, `refusal` or `recall`), `name`, `to`, `reason`, `recall`: what a use on the journey taught |
| `peers.message` | `from` (the sending client's character), `channel`, `data`: a message from this plugin in another client on the machine |
| `plugin.loaded` | none: the plugin's entry file has run |
| `context.begin` | `context` (`world`, `pregame` or `chargen`), `previous` (the context before; nil for a plugin just loaded): the plugin is now active |
| `context.end` | `context` (the one it was active in), `next`: the plugin is no longer active, and has let go of the character |

A host that sees no plugin listening for an event does not compare what the event needs, so
subscribing late does not report what happened before.

`chat.line`'s `kind` is one of `default`, `all_channels`, `speech`, `tell`, `tell_sent`, `system`,
`combat`, `magic`, `channel`, `channel_sent`, `social`, `social_sent`, `emote`, `advancement`,
`abuse`, `help`, `appraisal`, `spellcasting`, `allegiance`, `fellowship`, `world_broadcast`,
`combat_enemy`, `combat_self`, `recall`, `craft`, `salvaging`, `error` (types 0 to 26, in that
order), or the line's type number when it has none of these names. Lines of kind `tell` and
`tell_sent` reach every plugin
listening, as any other line does.

## Clicks on the map

A host that offers `map.clicks` tells each plugin active here that listens for `map.clicked`, at
the next frame, when the player presses the game's world map: the place under the pointer, in the
map coordinates markers and movements take, with the button and the modifiers held.
`markers.arrow:set{ position = e.position }` points at the spot clicked, and a map marker at
`e.position` is drawn on the pixel pressed. On a host without `map.clicks`,
`on("map.clicked", …)` is an error: a plugin that can do without it asks `host.has("map.clicks")`
first.

- The press is told as well as, never instead of, what the map itself does with it.
- It is no gesture: the map is the game's surface, not the plugin's own, so it gives no
  activation ([permissions](permissions.md#gestures-and-automation)) and needs no permission.
  Every plugin listening hears it, as each hears the selection change.
- A press where the map shows no place in the world (its frame, its margin past the world's
  edge) is told to no one, nor is one on anything standing over the map.

The desktop and browser clients tell the presses on the world map of every interface: the modern
interface's map page, the classic interface's map page (which takes no middle button), and
Horizon's map window, where the pad's A at the map's cursor is a left press there. The headless
client has no map and offers no `map.clicks`; the conformance driver presses the map with a
step's `map_click`.

## Where a plugin runs

A plugin is active only in the parts of the session its manifest names in `contexts`
([manifest](manifest.md)): `world` (a character is in the world, portal space included; the default),
`pregame` (the login and character screens, entering the world, after a disconnection) and
`chargen` (the character creation screens). The session is in exactly one context at a time.

- A plugin becomes active at the first frame of one of its contexts (or the first frame after
  it loads in one) and hears `context.begin`; it stops being active at the first frame of a
  context it does not name and hears `context.end`.
- While it is not active it hears only the **lifecycle events** (`plugin.loaded`,
  `context.begin`, `context.end`, `session.pregame`, `session.login`, `session.login_failed`,
  `session.logout`, `session.disconnect`, `session.world_policy`) and the settling of its own
  actions (`action.completed`, `action.failed`). It hears no other event, its timers wait (a
  repeating timer fires once when it is active again, then keeps its period), its commands are
  answered by the host with a line saying it is not active here, and its windows, HUDs and
  markers are hidden; they come back as they were when it is active again.
- When it stops being active the host lets go of the character for it, as for a suspended
  plugin: its queued and running actions fail `stopped` (an attack of its is cancelled), and
  its movement and the run lock it set end. Its HTTPS requests and pre-game requests go on.
- `session.info().context` is the context the session is in.

Where a value depends on the game's data files (a vital's maximum, for example), a host without
them reports the event only when it can compute it.
