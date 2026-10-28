# The Lua interface

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

Every name below is a global of the plugin's VM. The tables are read-only. A function given the
wrong arguments, or one whose permission the plugin lacks, raises a Lua error.

## Events

| function | what it does |
|---|---|
| `on(name, fn)` | calls `fn(event)` for each event `name` (see [events.md](events.md)); answers `fn` |
| `off(name, fn)` | stops that; answers whether it was registered |

## Output

| function | what it does |
|---|---|
| `print(...)` | a line in the chat window under the plugin's name, `[XP Meter] …` (the values converted with `tostring`, separated by tabs; on one line, cut to 200 characters); as many a frame as `chat.print` may print, the rest in the client's log |
| `chat.print(text [, style])` | a local line in the chat window; nothing is sent. `style` is a chat kind (`default`, `system`, `advancement`, `help`, `magic`, `combat`, …; see [events.md](events.md)); `default` when absent. Needs `chat.print` |
| `chat.hide(line)` | in a `chat.line` handler: this frame's line is not shown. `line` is the event table or its `line` field. Needs `chat.filter` |

## Commands

| function | what it does |
|---|---|
| `commands.add(name, help, fn)` | claims `/name` (and `@name`): when the player types it, `fn(args, words)` runs, `args` being the rest of the line and `words` it split on spaces and tabs. A name is letters, digits, `_` and `-`, compared without case. A command the client already has, `plugin`, or one another plugin claimed cannot be claimed |
| `commands.remove(name)` | gives it up |

The host's own command is `/plugin`: on its own (or `/plugin show`) it opens the plugin panel,
which lists the plugins and their state, says why the chosen one stopped, and has four buttons:
**Enable** (lit for a plugin that is not running), **Disable** (lit for one that is enabled),
**Reload** (lit for one running) and **Reload All** (lit while any runs). `/plugin list` and
`/plugin reload <id>` do the same in the chat window, and `/plugin enable <id>` and
`/plugin disable <id>` enable or disable a plugin the client found (one the launcher offers starts
enabled, any other is listed `disabled` until enabled, and the host remembers which are enabled
and which the player disabled; [install-and-update](guide/install-and-update.md)). A plugin named
to load (`--plugins`) is enabled again after a fault, but not disabled. `/plugin policy` tells the
world's policy in force, and `/plugin huds unlock` and `/plugin huds lock` unlock and lock the
plugins' HUDs for the player to move ([ui.md](ui.md#huds)).

The chat says when a plugin is loaded, reloaded, unloaded, enabled or disabled, one line each by
its manifest's name: `[Plugins] XP Meter loaded.` A plugin enabled says `enabled.` rather than
`loaded.`, and one disabled `disabled.` rather than `unloaded.`.

## Tasks

Timers run on the host's frame clock, so a replay fires them on the same frames every time.

| function | what it does |
|---|---|
| `task.every(ms, fn)` | calls `fn()` every `ms` milliseconds (at least 50); answers an id |
| `task.after(ms, fn)` | calls `fn()` once, after `ms`; answers an id |
| `task.cancel(id)` | cancels a timer |
| `task.spawn(fn, ...)` | runs `fn(...)` as a task (a coroutine) that may sleep |
| `task.sleep(ms)` | inside a task: waits `ms`, then carries on |

A repeating timer that falls behind fires once and resumes its period from then: it does not burst.
A plugin holds at most 256 timers at once (a sleeping task holds one); past that, starting another
fails. A timer due while the plugin is past its share of the frame (see the
[README](README.md#how-a-plugin-runs)) fires at the next frame instead, in its turn.

## The stores

A plugin has three stores: `store.global` (everywhere), `store.world` (this world, whichever
character) and `store.character` (this character on this world and account). `store.X.key` reads a
value, `store.X.key = value` writes one, and `nil` deletes it. `store.global` opens when the plugin
loads; the other two open at login (using them before is an error). All are written out at logout,
at unload and every 60 seconds. Values are nil, booleans, numbers, strings, and tables of them: a
table whose keys are `1..n` is a list, any other a map with string keys. Each store holds at most
16 MiB (a host may set less); a write that would take it past that is an error and changes
nothing.

`files.package(path)` answers the text of a file in the plugin's own package (a loot profile, a
data table), or nil; `files.package_list()` the path of every file in it.

**The data folder.** Each plugin has a data folder of its own, where the player puts files for it
(a VTank route, loot profile, settings profile or meta) and where it saves files for the player.
`files.data(path)` answers a file's text, or nil; `files.data_list()` every file's path;
`files.save_data(path, text)` writes one (at most 4 MiB, 1,000 files; paths are `/`-separated and
stay inside the folder). It needs no permission: a plugin reaches only its own folder. The desktop
keeps it at `plugin-data/<id>` in the settings folder; the headless client at `--plugin-data
<dir>/<id>`; a host with nowhere to keep it answers nil and refuses the write.

**A file the player picks.** `files.import{ prompt, extensions }` is a promise of `{ name, text }`
(at most 4 MiB): the host asks the player each time, in a dialog of the interface
([permissions](permissions.md#a-file-the-player-picks)), listing the files of the player's import folder
(`import/` in the host's own data folder, `plugin-data/dereth.plugins/import/` on the desktop) with
the extensions asked for. The player picks one and gives it, or refuses (the promise fails
`refused`); with no file to give, the question says where to put one, and only a refusal answers
it. Requests wait their turn, one a plugin: a second while one waits fails `a file is already asked
for`. After a refusal the plugin's next request waits 30 seconds before it is put, doubling with
each refusal in the session, and after five refusals every request fails `refused: no more files
are asked for it until it is reloaded`. Any plugin may ask, and it learns only the file the
player gave.

**The game's own data** (`game.data`, a host with the game's data files): `game.data.landblock(id)`
is a landblock (`0xXXYY`, or any cell id inside it) as `{ id, heights, terrain, roads, cells,
buildings, objects }`: its 81 vertices, 9 by 9 and 24 metres apart (index `x * 9 + y`), as heights
in metres, terrain types and road flags, and the ids of the dungeon or building cells inside it.
`game.data.cell(id)` is one of those cells as `{ id, environment, position, portals, visible,
floor, bounds }`: its place in the landblock (`{ x, y, z }`, metres), each portal's `{ to,
outside }`, the cells seen from it, its floor as polygons of `{ x, y }` in the landblock's frame,
and its box `{ min, max }`. Both are nil for what the data does not hold. They are what a dungeon
or landscape map is drawn from (on a `canvas`).

`require("dereth.profiles").usd(text)` reads a VTank settings profile (`.usd`): each of its tables
by name (`Settings`, `MyMonsters`, `AssistItems`, `BuffedItems`, …), as rows of columns.

## Actions

An action asks the game to do something, through the same requests the interface makes for the
same gesture, so it passes the same checks and sends the same messages. Each answers a
**promise** at once and is carried out in turn: the host sends one request at a time, for every
plugin, and only when the client is not busy with an item. The promise settles when the client
sees how the action ended, or fails when its time runs out. [actions.md](actions.md) lists every
action, what it sends, what ends it and its timeout.

| name | what it does |
|---|---|
| `await(p)` | inside a task: waits for promise `p`; answers its value, or raises its failure |
| `pawait(p)` | the same, answering `true, value` or `false, reason` instead of raising |
| `p:next(on_done [, on_failed])` | calls one of them when `p` settles (at once if it has) |
| `p.state` | `pending`, `completed` or `failed`; `p.value` and `p.reason` once settled |

| namespace | actions |
|---|---|
| `act` | `select(id)`, `use(id)`, `use_on(item, target [, { purpose = "heal" }])`, `appraise(id)` |
| `items` | `move(id [, container [, slot]])`, `split(id, count [, container])`, `merge(id, target [, count])`, `drop(id)`, `give(id, target [, count])`, `wield(id)`, `unwield(id)`, `inscribe(id, text)`, `salvage(ids [, tool])` |
| `vendor` | `buy{ {id =, count =}, … }`, `sell(ids)`, `close()`, `fill_components([max_price])` |
| `trade` | `open(player)`, `add(id)`, `accept()`, `decline()`, `reset()`, `close()` |
| `social` | `fellowship_create(name [, share_xp])`, `fellowship_quit([disband])`, `fellowship_recruit(id)`, `fellowship_dismiss(id)`, `fellowship_leader(id)`, `fellowship_openness()`, `friend_add(name)`, `friend_remove(id)` |
| `character` | `raise_skill(id)`, `raise_attribute(id)`, `raise_vital(id)`, `set_option(name, value)` |
| `magic` | `cast(spell [, target [, { purpose = "heal" }]])`: a Life spell cast as a heal counts as healing (`auto.heal`), not buffing |

A container is the player (the main pack) when it is not given. `act.appraise`'s value is the
appraisal (see `obj:appraisal()` below). `act.use_on` uses one item on another as the game's
interface for plugins does: a kit on the character, a lockpick on a door, a mana stone on an
item, and an item the character wields where it is, as a caster's own spell (a wand's Stamina to
Mana, a Healer's Heart's heal) is cast; `purpose = "heal"` counts a wielded item's restoring spell
as healing (`auto.heal`), as `magic.cast`'s does. Who may act, and when, is in
[permissions.md](permissions.md#gestures-and-automation).

Combat (`combat.set_mode`, `combat.attack`, `combat.stop`), metas, VTank meta files, expressions,
and the heal and debuff helpers are in [automation.md](automation.md).
Movements of the character (`nav.move_to`, `nav.follow`, `nav.face`, `nav.autorun`, `nav.stop`)
are in [nav.md](nav.md); pre-game operations (`session.select_character`, `session.logout`,
`session.quit`) in [pregame.md](pregame.md); `net.fetch` in [peers.md](peers.md). Each answers a
promise the same way.

## Chat sent

`chat.say(text)` says a line and `chat.command(line)` types a command (a leading `/` is added when
missing), as the player would in the chat bar; both need `chat.send`, and a plugin sends at most
three lines in any three seconds. A command another plugin claims, or the host's own `/plugin`, is
refused: call that plugin's function instead. The verb is read as the client will read the line,
after it is narrowed to the client's code page (a full-width `／` or letter becomes its plain
form) and with any commas after it dropped. And a line a plugin sends is never taken for a
command, even its own or one claimed after the line went: only the player's typing is.

## Loot profiles

`require("dereth.loot")` is the host's loot profile library: the `.utl` rule files players already
have, read and matched by the host.

| name | what it does |
|---|---|
| `loot.load(text)` | reads a profile's text (`files.package("rules.utl")`); raises when it is not one |
| `profile:classify(item)` | the first rule the item meets, as `{ name, action, keep, count }` (`action` is `keep`, `salvage`, `sell`, `read`, `keep_up_to`, `user1` … `user5`; `count` for `keep_up_to`), or nil: the item is not looted |
| `profile:needs_appraisal(item)` | whether the profile can only decide the item after an appraisal |
| `profile:rules()` | the rules, in order: each `{ name, action, count, conditions }`, `count` the number a `keep_up_to` rule keeps and `conditions` its requirements in plain words (`its value at least 2500`, `its class is gem`) |
| `profile:salvage_combine()` | the profile's salvage combining, or nil when it has none: `{ default, materials }`, `default` the workmanship ranges for any material, `materials` a list of `{ material, ranges }` (a material's number, as `obj.material` gives it, and its own ranges), each range `{ from, to }` in whole workmanship, both counted (a profile's `1-6,7,8` is `{1, 6}, {7, 7}, {8, 8}`) |

`item` is an object or its id. Rules are tried in file order. A requirement the host cannot read
(the item colour and palette ones, and those computed from the character's buffs) never matches.

## Reading the game

Reads are synchronous and show the game as of this frame.

**`game.player`** (fields are nil before login):

| field | value |
|---|---|
| `id` | the player's object id |
| `name` | the character's name |
| `level` | the character's level |
| `health`, `stamina`, `mana` | `{ current, max }`, where the host has the data to compute them |
| `xp` | `{ total, unassigned, to_level }` (`to_level` nil without the experience table) |
| `luminance` | `{ earned, max }` |
| `burden` | the carried load, as a fraction of capacity |
| `coords` | `{ x, y }`, the map coordinates |
| `heading` | degrees |
| `outside` | whether the player is outdoors |
| `combat_mode` | the combat mode number |
| `vitae` | the vitae fraction |
| `skills` | the character's skills, where the host has them: `{ id, name, training, base, current }` each (`training` is `untrained`, `trained` or `specialized`; `current` counts enchantments) |

| `enchantments` | the character's enchantments: `{ spell, name, beneficial, remaining, category }` each (`remaining` in seconds, nil when it does not run out) |
| `attributes` | the six attributes, `{ id, name, base, current, cost }` each (`strength`, `endurance`, `quickness`, `coordination`, `focus`, `self`; `current` counts enchantments; `cost` the experience to raise it one) |
| `vital_stats` | the vitals' maximums as attributes, `{ id, name, base, current, cost }` each (`health`, `stamina`, `mana`) |
| `fellowship` | the fellowship, or nil: `{ name, leader, share_xp, even_split, open, locked, members }`, each member `{ id, name, level, xp_percent, health, stamina, mana }` with each vital `{ current, max }` |
| `allegiance` | the allegiance, or nil: `{ name, members, followers, rank, monarch, patron, vassals }`, each person `{ id, name, online, rank, passed_up }` |
| `monarch` | the monarch's id, or nil |
| `titles` | `{ current, current_name, list }`: the title shown, and each earned `{ id, name }` |
| `augmentations` | the augmentations held, by name (`bonus_xp`, `innate_strength`, `resistance_fire`, `luminance_damage_rating`, …) to how many times; those not held are absent |
| `quests` | the quests the client tracks (contracts): `{ id, name, status, stage, description, contact, timed, repeat_remaining }` each (`repeat_remaining` seconds until it can be done again, when the client knows) |
| `spell_bars` | the eight spell bars, each a list of spell ids in bar order |
| `options` | the character's options by name (`auto_repeat_attack`, `view_combat_target`, …: the names `character.set_option` takes) to true or false |
| `skill_credits` | the skill credits not spent |
| `info` | `{ deaths, chess_rank, fishing_skill, encumbrance, capacity }`, where the host has them |

**`game.world`**:

| function | answers |
|---|---|
| `game.world:get(id)` | the object with that id, or nil |
| `game.world:find{ ... }` | every known object matching all of `name` (exact), `name_contains` (without case), `container` (an id), `wielder` (an id), `type` (the object type number), `wcid` (its kind's number), `class` (a class name below, without case, or its number), `monster` (true: creatures the character can attack that are not players), `within` (metres from the character) and `where` (a list of property conditions, below); in id order, or nearest first with `sort = "distance"`; at most `limit` of them |
| `game.world:find(filter, pred)` | as `find`, keeping only the objects for which `pred(obj)` is true (the filter may be left out: `game.world:find(pred)`) |
| `game.world:first(filter, pred)` | the first object `find` answers, or nil |
| `game.selection()` | the selected object, or nil |
| `game.player_int(key)` | the character's own integer property `key` (a game property number), or nil |
| `game.inventory.items()` | every item the player holds (main pack, side packs, worn), as objects |
| `game.inventory.open_container` | the id of the corpse or chest whose contents are open, or nil |
| `game.inventory.free_slots` | the free item slots in the main pack |
| `game.inventory.packs()` | the character's packs, the main pack first and then the side packs in the order the main pack shows them: `{ id, name, capacity, used }` each (`used`: the items in it; the side packs are not items of the main pack) |
| `vendor.info()` | the open vendor, `{ id, stock, buy_list, sell_list }` (rows `{ id, name, price, amount }`), or nil |
| `trade.info()` | the open trade, `{ partner, partner_name, mine, theirs, accepted, partner_accepted }`, or nil |
| `magic.spell(id)` | `{ id, name, icon, known, school, level, category, description, difficulty, mana, duration }` (`icon`: the spell's icon in the game's data) (`difficulty`: the skill the spell is pitched at; `mana`: its base mana cost; `duration`: how long it lasts, in seconds, nil for a spell that does not run out) (`category`: the spell table's category of the spell, which its levels and its higher namesakes share, and kindred spells too; nil where there is none) (school 1 War, 2 Life, 3 Item, 4 Creature, 5 Void), or nil where the host has no spell table |
| `magic.known([from, count])` | the character's spell book: `{ id, name, school, level, category, difficulty, mana }` for each spell it knows that the spell table has, in id order; with `from` and `count`, only `count` of them from the `from`th (counting from 1). A character can know thousands of spells: read the book a page at a time, a page a frame, to stay within the time budget |
| `regex.match(pattern, text [, ignore_case])` | the first match of a regular expression, `{ text, groups, named }`, or nil (the dialect: [automation.md](automation.md#patterns)) |

### Objects

An **object** reads through to the game each time, so a handle to an object that has gone reads
nil: `obj.id`, `obj.name`, `obj.icon`, `obj.type`, `obj.wcid`, `obj.container`, `obj.wielder`,
`obj.stack`, `obj.value`, `obj.burden`, `obj.selected`, and `obj.class`: `Player`, `Monster`,
`Npc`, `Vendor`, `Corpse`, `Door`, `Portal`, `Lifestone`, `MeleeWeapon`, `MissileWeapon`,
`WandStaffOrb`, `Armor`, `Clothing`, `Jewelry`, `Food`, `Money`, `Container`, `Gem`,
`SpellComponent`, `Foci`, `Key`, `TradeNote`, `ManaStone`, `HealingKit`, `Lockpick`, `Salvage`,
`Ust`, `Services`, `Book`, `Scroll`, `Journal`, `BaseCooking`, `BaseAlchemy`, `BaseFletching`,
`CraftedAlchemy`, `CraftedFletching`, `Misc` or `Unknown` (`obj.class_id` is its number, as loot
profiles store it).

| method | answers |
|---|---|
| `obj:int(key)`, `obj:float(key)`, `obj:str(key)`, `obj:bool(key)` | a property by its number: what every client is told (name, value, burden, material, …) and what the last appraisal added. Integer keys from `0x0D000000` and float keys from `0x0A000000` name figures kept outside the property tables, as loot profiles number them (`0x0D000006` the stack size, `0x0D000022` a weapon's maximum damage, `0x0A00000B` its variance, …) |
| `obj:int64(key)`, `obj:did(key)`, `obj:iid(key)` | a 64-bit integer, data-id (a number naming game data: an icon, a setup, a palette) or instance-id (another object) property: the object's own qualities (the client keeps them for the player's character) and then the last appraisal (which carries no instance ids) |
| `obj:prop(type, key)` | any property by its type's name: `int`, `int64`, `float`, `string`, `bool`, `did` or `iid` |
| `obj:health()` | what the client knows of its health: `{ current, max, fraction }` for the player, a fellow (the fellowship's figures) or a creature it has appraised; `{ fraction }` for the selected object's health bar alone; nil when it knows nothing |
| `obj:appraisal()` | the last appraisal, or nil: `{ success, ints, int64s, floats, strings, bools, spells }`, each table keyed by property number, `spells` a list of spell ids |
| `obj:properties([type])` | every property the client knows of it, `{ ints, int64s, floats, strings, bools, dids, iids }`, each table keyed by property number: what its public description told every client, under the game's own property numbers (its type 1, burden 5, stack 12, value 19, icon 8, container 2, …), then what the last appraisal added, then its own qualities (the character's); with `type` (`int`, `string`, …), that type's table alone; nil once it is gone |

Its fields also give `obj.uses` and `obj.max_uses` (a kit's or a tool's), `obj.max_stack`,
`obj.slot` (where it is worn or wielded), `obj.material`, `obj.workmanship`, `obj.plural`,
`obj.monarch`, `obj.spells` (the spell ids its last appraisal listed) and `obj.appraised`
(whether the client holds an appraisal of it).

A **property condition** in `find`'s `where` is `{ type, key, op, value }`: `type` one of `int`,
`int64`, `float`, `string`, `bool`, `did`, `iid`; `key` the property number; `op` one of `==`,
`~=`, `<`, `<=`, `>`, `>=`, `contains` (a string, without case), `exists` and `missing` (no
`value`). A condition on a property the client does not know is false, except `missing` and `~=`.

```lua
-- Every item in the pack worth a thousand or more, nearest first.
local rich = game.world:find{ container = game.player.id, sort = "distance",
  where = { { type = "int", key = 19, op = ">=", value = 1000 } } }
```

## The host

| name | value |
|---|---|
| `host.api` | the API version this host implements, `"0.7"` |
| `messages.version` | the [message stream](messages.md)'s own version, `1` |
| `game.player.busy` | whether the character is busy (an item request or a cast outstanding) |
| `game.player.game_time` | the game's calendar: `{ ticks, year, month, month_name, day, hour, hour_name, day_fraction, is_day, minutes_until_day, minutes_until_night }` (a day is 2 h 7 min; daytime runs from Dawnsong to the end of Warmtide-and-Half) |
| `policy.info()` | the world's policy in force: `{ published, rules, peers_local, notes }` (see [policy.md](policy.md)) |
| `session.info()` | the session: `{ phase, characters, world, account, portal_space, context }` (see [pregame.md](pregame.md)) |
| `host.name` | `dereth-desktop`, `dereth-headless`, … |
| `host.version` | the host's own version, for reports, never for feature tests |
| `host.has(capability)` | whether the host offers it |
| `permissions.has(name)` | whether the plugin may use what the permission covers: true for each but `net.http`, which its manifest must name ([permissions](permissions.md)) |
| `permissions.activation()` | seconds left of the activation the player's last gesture gave the plugin (0 when none is live) |

## Lifecycle hooks

A plugin may define two globals:

- `on_unload()`: called before the plugin is unloaded or reloaded. What it returns (a plain value)
  is handed to the next instance.
- `on_reload(state)`: called in the new instance after a reload, with what `on_unload` returned.

## Time

`os.clock()` is the seconds since the plugin loaded, on the frame clock. `os.time()` is the Unix
time the game shows. `os.date(fmt [, t])` formats `t`, or the game's time. `os.difftime` is Lua's.
