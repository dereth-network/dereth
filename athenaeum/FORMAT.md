# The world format

How an Asheron's Call 2 world is written: the world file, its packs, the rows of each kind, their
keys and references, and where each row came from. It is the format of the world database
Yalain serves (`yalain-world`) and of every world made over it, by hand or with Athenaeum.
`athenaeum validate <world>` (and `yalain world check`) holds a world to all of it.

A test of Athenaeum's reads this page and checks it against the rows the server reads: every
row kind has a section here, and each section's table names exactly the fields its kind has.

Athenaeum's editor places and changes `[[object]]`, `[[camp]]`, `[[place]]` and `[[clearing]]`
rows, changes, adds and takes away `[[pet]]` and `[[pet_limit]]` rows, and writes a storekeeper's
`[[keeper]]` and `[[store]]` rows when its shop is given a title; it writes `[[remove]]`
when a row of the database is taken away. Every other kind is written by hand, and the editor takes
in a file changed on disk while it is open. [`sample/`](sample/) is a small world over the
database to start from.

## Files

```text
world.toml                         the world: key, game, era, packs in load order, overrides, base
world.lock                         ids given to authored rows, kept across builds (a shop's copy under [store])
<pack>/pack.toml                   name, version, game, authors, license, description
<pack>/server/landblocks/XXYY.toml [[object]], [[camp]] and [[clearing]] rows standing in landblock XXYY
<pack>/server/places.toml          [[place]]
<pack>/server/loot.toml            [[loot]]
<pack>/server/stores.toml          [[keeper]], [[store]], [[sale]]
<pack>/server/quests/QQQQQQQQ.toml [[quest]], [[step]] of one quest
<pack>/server/gives/TTTTTTTT.toml  [[give]] of one template given to
<pack>/server/talk/TTTTTTTT.toml   [[talk]] of one template
<pack>/server/crafting.toml        [[craft]], [[tool]], [[spellbinding]], [[refining]], [[recipe]],
                                   [[combination]]
<pack>/server/mines.toml           [[mine]]
<pack>/server/maps.toml            [[map_region]], [[map_recipes]]
<pack>/server/story.toml           [[scene]], [[shard]]
<pack>/server/butchery.toml        [[butchery]]
<pack>/server/weapons.toml         [[weapon]]
<pack>/server/death_spawns.toml    [[death_spawn]]
<pack>/server/death_treasures.toml [[death_treasure]]
<pack>/server/pets.toml            [[pet]], [[pet_limit]]
<pack>/server/plants.toml          [[plant]], [[plant_cross]]
<pack>/server/worn.toml            [[worn]]
<pack>/server/pools.toml           [[treasure_pool]]
<pack>/server/effects.toml         [[effect_name]], [[effect_rule]]
<pack>/server/events.toml          [[event]], [[tally]]
<pack>/server/values.toml          [[treasure_value]]
<pack>/server/routes.toml          [[route]]
<pack>/server/areas.toml           [[area]]
<pack>/server/kingdoms.toml        [[points_bonus]]
<pack>/server/popups.toml          [[popup]]
<pack>/server/treasure.toml        [[treasure_rules]], [[treasure_sort]]
<pack>/server/treasure_words.toml  [[treasure_word]]
<pack>/server/treasure_looks.toml  [[treasure_look]]
<pack>/server/treasure_potions.toml [[treasure_potion]]
<pack>/server/treasure_families.toml [[treasure_family]]
<pack>/server/items.toml           [[item_use]], [[book]], [[start_item]], [[interaction]]
<pack>/server/puzzles.toml         [[object_states]], [[sequence]], [[lever]], [[offering]]
<pack>/server/creatures.toml       [[concealing]], [[npc_table]], [[friend_table]], [[creature]]
```

`[[object]]`, `[[camp]]` and `[[clearing]]` rows must be written in their landblock's file. Every
other kind may be written in any TOML file under a pack's `server/`, which also takes `[[remove]]`
rows; the files above are where the database and the editor write them. One file per landblock and
per quest keeps changes small: two people filling different towns never touch the same file.

```toml
# world.toml
format_version = 1
key = "arwic-plus"
game = "ac2"
era = "eor"
packs = ["fill-arwic"]                # load order

[base]                                # the world database under every pack
db = "yalain-world"                   # add version = "vX.Y.Z" to keep one release

[[override]]                          # where two packs set one item, this pack wins
pack = "fill-arwic"
over = "fill-holtburg"
# items = ["camp c-4586@77B8"]        # only these items (all of them when left out)
```

```toml
# <pack>/pack.toml
format_version = 1
name = "fill-arwic"                   # lowercase letters, digits and hyphens
version = "0.1.0"
game = "ac2"
authors = ["you"]
license = "CC0-1.0"
description = "People of Arwic's market."
```

A world's rows replace the database's row of the same item, and `[[remove]]` takes one away.

## Values

- **Data ids** are integers, written in hexadecimal: `template = 0x470031E6`. They name records of
  the install: templates `0x47...`, cells (`0xXXYYCCCC`, landblock and cell), saved states
  `0x6F...` and `0x70...`, string tables `0x25...`, pictures `0x41...`.
- **Enum values** are integers whose enumeration is fixed by the field: `profile` is always a
  generator profile.
- **Identifiers** of the install may be written as text beside their number (`profile_name =
  "GP_AnExampleCamp"`, `id_name = "QuestAnExample"`, a place's `destination =
  "dng_btd_ironlinvak"`): letters, digits, `_`, `:`, `.`, `-`, at most 80. A build checks that each
  is the install's own for its number.
- **String references** point at the install's text: `{ table = 0x25002AFC, string = 0x2824938C }`.
- **New prose** a pack writes goes in a field ending in `_text` (`name_text`, `greeting_text`,
  `tell_text`, `line_text`), at most 200 characters, beside the string reference it stands in for.
  The reference is used when there is one; with only `_text`, the text is said as it is (the same
  in every language, with nothing filled in). The game's own text is never copied into a `_text`
  field.
- **Token names**: a line's `line_name` is the name of the string token its string id is (such as
  `ID_Begin2`); every token is computed from its name, so a check holds the name to the id.
- **Places**: `cell` (the full cell id), `at = [x, y, z]` in meters within the cell's landblock
  (outdoors the landblock is 160 m on a side; z may be `"ground"`, the land's height there), and
  either `rotation = [w, x, y, z]` (scalar first) or `heading` (degrees clockwise from north).
  Outdoors `cell` may be left out: it follows from `at` (cells of 20 m, numbered from 1 along y
  within each x).

## Keys and references

Every row sets one **item**, named by its key:

| item | key |
|---|---|
| `object KEY@XXYY`, `camp KEY@XXYY`, `clearing KEY@XXYY` | the row's `key`, unique within its landblock in its pack |
| `place KEY`, `talk KEY`, `give KEY`, `route KEY`, `event KEY`, `tally KEY`, `death_treasure KEY`, `object_states KEY`, `sequence KEY`, `lever KEY`, `offering KEY` | the row's `key` |
| `loot 0xVALUE` | the death-loot profile's value |
| `keeper 0xTEMPLATE` | the storekeeper template |
| `store KEY` | the shop's `key`, unique in the world |
| `sale 0xSTORE ORDINAL` | the store template and the listing's ordinal |
| `quest 0xID` | the quest's id |
| `step 0xQUEST KEY` | the quest and the step's `key` |
| `craft 0xID` | the craft skill or recipe |
| `recipe 0xID` | the recipe's id |
| `combination KEY` | the row's `key` |
| `scene 0xID` | the scene's id |
| `shard 0xOBJECT` | the shard's world id |
| `butchery 0xPROFILE` | the butchery profile |
| `weapon NAME` | the weapon's name, as creatures' templates write it |
| `area XXYY` | the area's landblock |
| `treasure_pool NAME` | the pool's name |
| `pet 0xEFFECT` | the pet effect (or the contract's template) |
| `pet_limit 0xSKILL` | the summoning skill |
| `treasure_rules` | the world's one row |
| `treasure_sort SORT` | the sort: `weapon`, `armor`, `shield` or `jewelry` |
| `treasure_word SORT 0xWORD` | the sort and the word's string |
| `treasure_look 0xTEMPLATE` | the template |
| `treasure_potion KIND` | the kind's name |
| `treasure_family SORT ORDER` | the sort and the family's `order` |
| `treasure_value LEVEL` | the creatures' level |
| `mine 0xSKILL` | the mining craft skill |
| `map_region REGION` | the region's key |
| `worn 0xTEMPLATE 0xRACES 0xSEXES` | the template and the races and sexes it fits |

Numbers in an item are hexadecimal after `0x`, else decimal; `keeper 0x470033D0` and
`keeper 0x470033d0` are one item. Keys are lowercase letters, digits and hyphens. A row refers to
another by key: an object's `to` and a step's `to` name a place, a camp's `parent` a camp, a line's
`object` a person; `pack:key` reaches another pack's row, `key@XXYY` another landblock's.

Packs merge in load order. Two packs that set an item alike agree. A pack that sets an item an
earlier pack set differently wins only where an `[[override]]` says so (its `items` naming the
item, or `landblock XXYY` for everything in a landblock, or nothing for everything both set);
otherwise the build stops, naming both rows. A world's packs lie over the base it names: their
rows replace the base's row of the same item.

## Provenance

Every row has `prov`; a field whose source differs from its row's says so in `prov.fields`.

| `from` | means | required | optional |
|---|---|---|---|
| `capture` | measured from recordings of the live servers | `evidence` (an evidence handle), `ref` (opaque), `confidence` (`high`, `medium`, `low`) | `sample`: `sessions`, `creatures`, `corpses`, `views`, `trips`, `lines`, `visits` |
| `dat` | a record of the install | `record` | |
| `derived` | computed by a rule the database lists | `rule` | `record` |
| `authored` | written by a person | | `by`, `note` |

```toml
[[camp]]
key = "c-4586"
cell = 0x77B80040
at = [144.8, 144.7, 133.0]
radius = 5.0
population = 2
regen = 192.0
spawns = [{ template = 0x47002224, weight = 2 }]
prov = { from = "capture", evidence = "AC2-EVID-WDB-CAMPS-2017", ref = "camp 4586", confidence = "low", sample = { creatures = 2, sessions = 1 }, fields = { radius = { from = "authored" } } }
```

Athenaeum writes a new row as `{ from = "authored" }`. A change to a row of any other source keeps
the row's provenance and marks each field changed as a person's in `prov.fields`.

## Row kinds

Each kind's table lists all of its fields. **required** marks a field a row must have; a field
marked † is read from the install when left out.

### Where things stand

#### `[[object]]`

A person or an object of the world, in its landblock's file.

| field | | meaning |
|---|---|---|
| `key` | required | unique in its landblock |
| `kind` | required | `npc` (a person the server numbers) or `static` (an object with an id of its own) |
| `id` | statics | the world id; a new static is given one when the world is built |
| `template` | required | what it is |
| `cell`, `at`, `rotation`, `heading` | `at` required | where it stands, and which way it faces |
| `scale` | | how large it stands, in place of the scale its template gives; a build keeps the template's, with a warning, when it isn't above 0 |
| `name`, `name_text` | | a name the server gives it (a portal named for where it leads); else the template's |
| `to` | | the place using it takes a player to; a person says its line for a use as it does |
| `landings` | | a lifestone's recall spots: `{ cell, at, heading, seen }` |
| `scene` | | a door's scene: the `[[scene]]` a player plays on the way to `to`, in place of portal space |
| `locked` | statics | a locked door: from its front a plain use is refused and only its key opens it (it locks again as it closes); from behind it opens. Locked with `to`, a portal takes a player only on its key, refusing a plain use from every side: the key of a portal door (a door, or a portal that stands as one) is used up unless its kind keeps it, and what the door's kind names for its use moves its user's quests; a vault's glyph is used up. A static is locked with an `id` or without one |
| `lock` | | a locked door's or portal's key id: the id a key must carry to open it; none known: no key does |
| `route`, `route_start`, `route_direction` | people | the `[[route]]` a person walks (`key`, or `pack:key`), the waypoint it sets out toward (from 0) and `forward` or `backward` |
| `recall`, `summon` | statics | a portal a player who used it may recall to (`recall = true`) and summon a portal to (`summon = true`); left out, neither |
| `requires` | statics | the quest a player must hold to use it: `{ quest = Q, done = true }` (completed), `{ quest = Q, underway = true }` (under way at any phase), `{ quest = Q, phase = N }` (under way at that phase) or `{ quest = Q, had = true }` (failed, completed or under way); a use or a walk-in it keeps out is refused at once |
| `min_level` | statics | the least level a player must be to use it, over its template's; its examination names it |
| `hero` | statics | true: only a hero may use it; its examination says so |
| `kingdom` | statics | the kingdom a player must be of to use it, by the number a character's description names it: 1 (Shadow), 2 (Order) or 4 (Dominion) |
| `event` | | the `[[event]]` (its `key`) it stands for: it is in the world only while the event is on |
| `prov` | required | where it came from |

A chest whose template's class goes once closed (the install's chest that deletes itself on
closing: the prospectors' corpses, a treasure map's cargo chest, a nut hoard's stump) leaves the
world as its first opener closes it. One a row places is kept from the world until the server
starts again; to have such a chest come back now and then, let a `[[camp]]` keep it.

#### `[[camp]]`

A spawn camp, in its landblock's file.

| field | | meaning |
|---|---|---|
| `key` | required | unique in its landblock |
| `cell`, `at` | `at` required | its middle |
| `radius` | | outdoors, how far from its middle a creature appears (m) |
| `spots` | | indoors, the points creatures appear at: `{ cell, at }` |
| `population` | required | how many it keeps alive |
| `regen` | required | seconds from a death to its replacement |
| `spawns` | | what it makes: `{ template, weight }`; an item's template (one with a value, no creature) it keeps lying at its place, a new one `regen` seconds after the last is taken; a chest's template it keeps standing there, anyone's to open, a new one `regen` seconds after the last is gone (one of a kind that goes once closed: a prospector's corpse that comes back now and then, with a long `regen`) |
| `profile`, `profile_name` | | the generator profile it stands for |
| `parent` | | a master camp |
| `movement` | | how its creatures may move, over their template's: `stationary`, `turn-only`, `constrained-to-home` (the default), `constrained-to-master` or `free-range` |
| `wanders`, `wander_range`, `wander_chance` | | over their template's: whether they wander at rest (default true), how far from home (m, default 25), the chance a wander sets out when one falls due (default 0.1) |
| `homesick`, `perception` | | over their template's: the distance from home they keep to (m, default 40), how near a player must come to be noticed (m, default 20) |
| `uses_doors` | | over their template's: whether they open doors (default true) |
| `route` | | the `[[route]]` its creatures walk at rest (`key`, or `pack:key`), in place of wandering |
| `route_start`, `route_direction` | | the waypoint they set off toward (from 0) and which way they go on: `forward` (default) or `backward` |
| `name` | | the name its creatures are told by, examined by and their corpses named after, in place of their template's: a string reference |
| `event` | | the `[[event]]` (its `key`) it keeps its creatures for: none while the event is off |
| `prov` | required | where it came from |

#### `[[clearing]]`

Ground in a landblock kept free of what the land grows, in its landblock's file. Athenaeum builds
it into the world's data overlay (`athenaeum build`); the server has no use for it.

| field | | meaning |
|---|---|---|
| `key` | required | unique in its landblock |
| `at` | required | its middle: x and y in meters within the landblock, from 0 up to 160 |
| `radius`, `size` | one of them | a circle's radius, or a rectangle's sides `[x, y]` along its own axes, each above 0 and at most 240 m |
| `heading` | | turns a rectangle, degrees clockwise from north |
| `prov` | required | where it came from |

#### `[[place]]`

A destination, in `server/places.toml`.

| field | | meaning |
|---|---|---|
| `key` | required | its name, which what leads to it uses |
| `cell`, `at`, `rotation`, `heading` | `at` required | where a player arrives, facing which way |
| `named_by` | | templates whose destination names it: a portal's, or a surveyor's map's (a template is named by one place at most) |
| `destination` | | that name, as text: the name an effect that sends a player to a named place looks it up by (one place to a name) |
| `row`, `holds` | | a room of a row of like rooms, and the cell its creature stands in: a step that leads to any room of a row sends a player to one of them, free of players and with its creature standing if it can |
| `prov` | required | where it came from |

#### `[[route]]`

Places creatures (and people that name it) walk at rest in turn. Several camps may share one
route, each with its own start; a creature that gives up a fight goes back to its route's nearest
waypoint.

| field | | meaning |
|---|---|---|
| `key` | required | its name |
| `waypoints` | required | `{ cell, at, rest }`, `rest` the seconds a creature stands there, else the route's |
| `shape` | | `back-and-forth` (turn back at either end, the default) or `loop` (on from the last to the first) |
| `gait` | | `walk` (the default) or `run` |
| `speed` | | meters a second for a creature of scale 1, when not the gait's own pace |
| `rest` | | seconds, for waypoints that set none; 0 when left out |
| `prov` | required | where it came from |

#### `[[area]]`

The kind of player conflict a landblock allows. A landblock no row names is a peace area.

| field | | meaning |
|---|---|---|
| `landblock` | required | `0xXXYY`, outdoors or a dungeon's |
| `conflict` | required | `peace` (no player may fight another), `controlled` (players of different kingdoms may fight, neutral players stay out of it), `kingdom-vs-kingdom` or `free-for-all` |
| `prov` | required | where it came from |

### People, quests and the story

#### `[[talk]]`

A line a person (or a creature) says, and when. One row per line; a person says the lines of its
rows whose moment and quest match, picking at random among several.

| field | | meaning |
|---|---|---|
| `key` | required | unique in its pack |
| `who` | required | the template that says it |
| `object` | | one placed person only (`key@XXYY`) |
| `line`, `line_text` | one of them | the line: a string reference, in the person's own string table, or new prose |
| `line_name` | | the token name of `line`'s string id |
| `role` | | what it does: `offer`, `remind`, `advance`, `accept`, `complete`, `fail`, `refuse-use`, `refuse-give`, `too-soon`, `use`, `near`, `idle`, `combat`, `death`, `teleport` (said as a person sends a map's holder where it leads), `refuse-map` |
| `how` | | `tell` (to the player, naming the speaker; the default), `say` (aloud, to everyone near), `emote`, `broadcast` (read by every player in the speaker's landblock and the eight around it) |
| `when` | required | the moment: `use`, `give`, `attack`, `death`, `near`, `idle` |
| `quest`, `update`, `phase` | | the quest update the line comes with (`bestow`, `advance` and its phase, `complete`, `fail`); a `remind` line names its `quest` and no `update` |
| `item` | | the item given, for a line said when one is |
| `order` | | its place in a speech of several lines (0 first) |
| `requires` | | tallies that must be as they say for it to be said: full or not yet full (`[{ tally = "key", full = true }]`), or a count at least and below so many (`{ tally = "key", at_least = 3, below = 4 }`, either or both) |
| `fills` | | a tally's key: the line's one variable is the tally's count as a percent of its `full`, rounded down. Only with `line`, a string of the install's that takes one variable |
| `full_line`, `full_line_text` | | with `fills`: what is said in the line's place once the tally is full |
| `prov` | required | where it came from |

#### `[[quest]]`

A quest's server values. Its name, description, icon and journal are its definition's in the
install; a name whose text the install doesn't hold (its table, or the table the name's entry sends
it on to, is missing) is said in the words of the quest's name in the install's quest enumeration
(`Quest_AnExampleHunt`: An Example Hunt).

| field | | meaning |
|---|---|---|
| `id`, `id_name` | `id` required | the quest's id, and its name |
| `state` | † | its definition |
| `challenge` | | its challenge level |
| `duration` | | seconds of the player's time in the world it may take, 0 for none |
| `timed_phase` | | the one phase the duration runs in; left out, every phase from its giving |
| `retry` | | seconds by the calendar before it is given again |
| `cancel_wait` | | seconds by the calendar after its player cancels it before it is given again; 300 when left out |
| `xp`, `xp_share`, `xp_share_level`, `xp_least`, `xp_most`, `xp_whole` | | what its completion pays: an amount, or a share of what the player's level takes to reach the next (paying no more at and above `xp_share_level`, between `xp_least` and `xp_most`); `xp_whole = true` pays an amount whole whatever the quest's challenge; left out, as the quest's setup in the install says |
| `qp` | | the quest points it gives |
| `cancellable` | | whether its player may cancel it |
| `visible` | † | false: kept for its player but never shown |
| `min_level` | | the least level a player is given it at |
| `fails_on_death` | | true: it fails when its player dies; false: not; left out, as its setup says |
| `fails_on_logout` | | true: it fails when its player leaves the world (logs off, or its client goes away); left out, as its setup says |
| `fails_on_teleport` | | true: it fails when its player goes through portal space (a portal, a door, a recall, a shard, any sending but a dead player's return); left out, as its setup says |
| `once` | | true: given only once; false: given again; left out, as its setup says |
| `fellows` | | true: a kill's move on it reaches the killer's fellows; false: only the killer |
| `companion_of` | | a quest whose holder this one goes with, below; another quest with a row |
| `prov` | required | where it came from |

`companion_of = Q` makes the quest a companion of quest `Q`: one its fellows take on to stand by
the player who carries `Q`. As a player's `Q` is done, this quest, under way, is done after it for
the player and for each of the player's fellows in the world, each paid as its own completion pays;
as `Q` fails (its player's death, its time running out, a step) or is cancelled, this one fails
after it for them alike. A fellow who has left the fellowship by then keeps its own as it is. The
companion is given as any quest is, usually by the step that gives `Q`, with `fellows = true`.

#### `[[step]]`

What moves a quest on, in its quest's file.

| field | | meaning |
|---|---|---|
| `key` | required | unique in its quest |
| `quest` | required | the quest it moves |
| `needs` | required | the phase it must be at; 0 for not under way |
| `trigger` | required | what the player does, below |
| `tell`, `tell_text` | | what the person says |
| `update` | required | `bestow`, `advance`, `complete`, `fail` (needs it under way) or `none` (it moves no quest, and only says, gives and sends) |
| `phase` | | the phase an `advance` moves it to |
| `xp`, `xp_share` | | experience it pays: an amount, or a share of what the player's level takes to reach the next |
| `least_level`, `most_level` | | the levels it runs for |
| `gives`, `counts` | | templates it gives, and how many of each in order (1 when left out) |
| `gives_one_of` | | one more item it gives, one of several picked by weight each time it runs: at least two `{ template, weight }`, each weight above 0 (`[{ template = A, weight = 3 }, { template = B, weight = 1 }]`: A three times in four) |
| `effects`, `text`, `effects_last` | | the effects it puts on the player, by their saved states; a text shown after them; `effects_last = true` tells the quest's update before them |
| `effects_quiet` | | true: its effects tell none of their own lines (a recipe or a skill they grant is granted in silence); its `text` is still told. Only with `effects` |
| `recipes` | | recipes it teaches, by id: the install's, or the world's `[[recipe]]` rows'; learned after its update and its effects |
| `requires` | | other quests it needs (`{ quest, phase }`, `{ quest, done = true }`, `{ quest, free = true }`, `{ quest, failed = true }`: failed, by a step or its time run out, and for a step of a quest under way given since that quest was, so a hidden timer given with the quest is waited on until it runs out), an event that must be on (`{ event = "key" }`), a tally that must be full or not yet full (`{ tally = "key", full = true }` or `full = false`) or whose count must be at least, and below, so many (`{ tally = "key", at_least = 3, below = 4 }`, either or both: `at_least` above 0 and at most the tally's `full`, `below` above `at_least`), a craft skill at a level or above (`{ craft = S, at_least = 200 }`, S the skill's saved state, or `craft = [S, ...]` for any one of several; the level a making reads, with what the player's effects add; a bestowing step a player meets all else of but this refuses with its person's `refuse-use` line for its quest, as one under the quest's least level is), acts that must be whole (`{ act = A }`: seven of the act's scenes held, its hidden ones aside; a scene of the act must be served) and one kingdom (`{ kingdom = "order" }`: `neutral`, `shadow`, `order` or `dominion`) |
| `adds` | | `{ tally = "key", by = N }`: as it runs it adds N (above 0) to the tally, never past its `full`, before anything it says |
| `takes` | | templates of items taken from the player's pack, which the player must hold |
| `to` | | a place it takes the player to, through portal space, for a use, a give, a take or a kill: a use of a person or an object, a give and a take tell the update first, an item used on an object and a kill as the player arrives; a recall the quest's definition names for the update then stays put. Without `to`, such a recall takes the player itself, the update told as the player comes out |
| `to_fellows` | | true, with `to`: the player's fellows go there too, each through portal space, wherever they are in the world (as the player goes; for a kill, at the kill); a fellow who is dead or already on its way somewhere stays. Where `to` is a room of a row, each goes to a room of its own. It moves no fellow's quest: `fellows` does that |
| `races` | | the races it is for, by number: 1 human, 2 Lugian, 4 Tumerok, 8 Drudge, 16 Empyrean; every race when left out |
| `fellows` | | true: the update is made for each of the player's fellows in the world as well |
| `wants` | | for a give, exactly how many of the item it wants: any other count is refused and handed back |
| `transforms` | | a give's step that makes its player a hero: `{ seconds, fx, behavior, text, name }` |
| `chance` | | above 0 and at most 1: the chance it runs each time it may; left out, every time |
| `escort` | | a creature it sets to follow the player for the quest, below |
| `prov` | required | where it came from |

`escort = { template, icon, until, far, portal_fails }` makes the creature of `template` follow the
player once the step has run, as one of the player's pets: its `[[pet]]` row (whose `effect` is the
same template) says how it follows and fights, and it is at its template's own level. The player's
list of pets shows it with `icon` (none when left out). Its death fails the quest, its player's death
included; so does the player getting more than `far` meters from it (never when left out), and,
with `portal_fails = true`, the player coming out of portal space somewhere else, but never from the
trip of a step that makes it and sends the player (otherwise it comes along, as a pet does). It goes, failing nothing, as the quest reaches phase `until` (above the phase
the step leaves it at), or, when left out, as the quest is done, failed or cancelled. The player
using it runs the steps a use of its template triggers. A player leaving the world takes it along
and has it again on entering while the quest still wants it. One follows a player for each quest.

| `trigger` | moves the quest when the player |
|---|---|
| `{ use = T }` | uses a person or an object of template T (or a held item of it) |
| `{ use = I, on = T }` | uses a held item of template I on an object of template T |
| `{ give = I, to = T }` | gives an item of template I to a person or an object of template T |
| `{ give_class = C, to = T }` | gives an item of any template of class C (a script package, by number: the template's own, or one it derives from) to a person or an object of template T; a give of the item's own template is looked for first, and the item is handed back |
| `{ arrive = T }` | arrives through a portal of template T |
| `{ arrive = T, from = O }` | arrives through the one placed portal O (its `id`), of template T |
| `{ take = T }` | takes an item of template T |
| `{ kill = T }` | kills a creature of template T |
| `{ kill = T, camp = K }` | kills a creature of template T that the one camp K made (`key`, or `key@XXYY`) |
| `{ kill = T, within = M }` | is within M meters (a whole number above 0) of a creature of template T as it dies, whoever kills it, in the world, out of portal space and alive |
| `{ done = Q }` | completes quest Q |
| `{ given = Q }` | is given quest Q, shown or kept hidden |

#### `[[give]]`

What a person or an object does with an item given it that moves no quest, in the file of the
template given to.

| field | | meaning |
|---|---|---|
| `key` | required | unique in its pack |
| `to` | required | the template given to |
| `item` | | the template given; left out, any item |
| `item_class` | | in place of `item`: a class (a script package, by number) any template of which it takes; a row naming the item is taken first, then one naming a class it counts in, then one for any item |
| `wants` | | exactly how many of it: any other count is refused, the stack handed back |
| `gives`, `counts` | | what it gives in its place, from the person, and how many of each |
| `gives_one_of` | | as a step's: one more item, one of several picked by weight |
| `effects`, `text` | | what it puts on the player, cast by the person, and a text after them |
| `effects_quiet` | | true: its effects tell none of their own lines; its `text` is still told. Only with `effects` |
| `fx` | | what the person or the object plays as it takes the item |
| `requires` | | as a step's, its events and acts aside; not met, the item is refused |
| `adds` | | as a step's: what taking the item adds to a tally |
| `prov` | required | where it came from |

#### `[[scene]]`

A scene of the story, as a client plays it.

| field | | meaning |
|---|---|---|
| `id` | required | the scene's id (an enumeration's value, such as `0x40000021`) |
| `act`, `number` | `act` required | its act and its place in the act |
| `name` | required | its name, a string reference |
| `hidden`, `playable` | | a scene no story page lists (default false); one a player may replay (default true) |
| `keyframes` | | its steps in order: `{ seconds, picture, music, voice, subtitle, tracks }` |
| `at_start` | | its place among the scenes every new character holds from its first entry |
| `prov` | required | where it came from |

#### `[[shard]]`

A story shard. Its destination is its `[[object]]` row's `to`.

| field | | meaning |
|---|---|---|
| `object` | required | the shard's world id |
| `scene` | required | the scene using it bestows |
| `quest` | | the quest using it moves |
| `prov` | required | where it came from |

### Stores

#### `[[keeper]]`

A storekeeper.

| field | | meaning |
|---|---|---|
| `template` | required | the storekeeper's template |
| `store` | † | the store template it opens |
| `shop` | | the `[[store]]` it opens, by key: that shop's store template, under the shop's title |
| `greeting`, `greeting_text` | | what it says as its store opens |
| `prov` | required | where it came from |

#### `[[store]]`

A shop of the world's own: a store template's stock, prices, sorter and portrait under a title of
its own. Its keepers sell the template's sales (and its `[[sale]]` rows name the template); the shop
window shows the title. The world's overlay holds the shop's copy of the template, under an id the
build gives it from `0x6F100000` to `0x6F7DFFFF` and keeps in `world.lock`; the server serves it to
players as they log in. The range ends below the world's recipes' (`[[recipe]]`, `0x6F7E0000` to
`0x6F7EFFFF`), so a shop's copy and a recipe never share an id; a `world.lock` pinning a shop
outside its range, or two shops to one id, is a problem.

| field | | meaning |
|---|---|---|
| `key` | required | unique in the world |
| `from` | required | the store template it copies |
| `name_text` | required | the shop window's title, the same in every language |
| `description_text` | | the line under the title; else the template's |
| `prov` | required | where it came from |

#### `[[sale]]`

What a store's listing shows that the store template lacks. Who a sale is for, and the stock
itself, are the store template's in the install.

| field | | meaning |
|---|---|---|
| `store` | required | the store template |
| `ordinal` | required | the listing's place in the store |
| `stack` | | how many it sells at once |
| `icon` | | `{ layer, picture, color }` |
| `product`, `cost`, `currency` | † | what it sells, for how much of what |
| `name`, `name_text` | † | its name |
| `prov` | required | where it came from |

### Loot and treasure

#### `[[loot]]`

What a dead creature leaves, by its death-loot profile. A creature's trophies are its template's
and are not rows. The same rows fill a chest from its generator profile at each opening, and a
corpse a creature's template names in place of the common one from the profile that corpse names,
beside the creature's own loot.

| field | | meaning |
|---|---|---|
| `profile`, `profile_name` | `profile` required | the death-loot profile |
| `counts` | required | the chance of 0, 1, 2, ... items |
| `items` | | `{ template, weight }`, what a draw picks from |
| `extras` | | `{ template, chance }`, items with a chance of their own |
| `level`, `value` | | a chest's: what it draws is treasure of that level, each piece worth a draw from the band `[low, high]` |
| `draws` | | treasure pools to draw from in place of `items`: `{ pool, weight, least, most }`, by weight among those whose window holds the item's level |
| `rare` | | `{ pool, chance }`, rolled once a corpse |
| `prov` | required | where it came from |

#### `[[treasure_pool]]`

A named pool treasure is drawn from. A pool may not lead back to itself, and a draw goes at most
eight pools deep.

| field | | meaning |
|---|---|---|
| `name` | required | the install's name for it, `GP_...` |
| `pools`, `items` | | `{ pool, weight, least, most }` and `{ template, weight, least, most }`: a draw takes one by weight among those whose window holds the item's level, and goes on down until it reaches a template |
| `race`, `own` | | `human`, `lugian`, `tumerok`, `drudge` or `empyrean`, and a share: a draw from its parent by a killer of that race takes this pool at that share outright |
| `prov` | required | where it came from |

#### `[[death_treasure]]`

A fixed item a creature leaves at its death, or a chest at an opening, at a chance of its own. A
chest's is rolled only at an opening its key paid for, unless the row names a quest: then it is
rolled at every opening whose opener holds that quest where it needs it, a key or none. A chest is
filled anew at each opening, so a quest's item is there again until the quest moves on (for
instance, as a `{ take = T }` step moves it).

| field | | meaning |
|---|---|---|
| `key` | required | its name |
| `template`, `camp` | one of them | every creature or chest of a template, or every creature a camp keeps (`key@XXYY`, or `pack:key@XXYY`) |
| `item` | required | the item's template |
| `chance` | required | above 0, at most 1 |
| `requires` | | a chest's alone: the quest its opener must hold, as a static's `requires` names it (`{ quest = Q, phase = N }`, `underway`, `done` or `had`); not with `camp`, and a creature never leaves a row that names one |
| `prov` | required | where it came from |

```toml
[[death_treasure]]
key = "relic-piece"
template = 0x47001234
item = 0x47005678
chance = 1.0
requires = { quest = 0x40000123, phase = 1 }
prov = { from = "authored", note = "the chest held it for a player at the quest's first phase" }
```

#### `[[treasure_value]]`

What treasure a creature of a level leaves is worth. A level with no row draws from the nearest
level that has one.

| field | | meaning |
|---|---|---|
| `level` | required | the creature's level |
| `values` | required | whole numbers, at least one: each treasure item carries one, drawn evenly |
| `prov` | required | where it came from |

#### `[[treasure_rules]]`

What every treasure item draws by, whatever its sort. A world has one row, or none and the
server's own.

| field | | meaning |
|---|---|---|
| `above_base` | | weights of an item's own least level standing 0, 1, 2, ... above its dropper's base |
| `above_base_override` | optional | the same for a dropper whose base is its loot quality override; left out, `above_base` |
| `most_level` | | the highest least level an item's own may be |
| `lore` | | `{ above, from, to }`: an item needs lore to use only when its dropper's base is above `above` |
| `potion_tiers` | | the least dropper level of each of a treasure potion's five tiers |
| `potion_spellcraft` | | `[least, most]`, a potion's spellcraft, drawn evenly |
| `look` | | `{ glow_most, round_chance, round_from, round_tenths }`: how glows and colors are drawn |
| `prov` | required | where it came from |

#### `[[treasure_sort]]`

The numbers one sort of treasure item draws by. A world gives all four sorts or none. Each sort has
its own fields beside `counts`: weapons `damage_share`, `vigor` and `delays`; armor `delays`,
`rating_share`, `rating_spread` and `slot_chance`; shields all of those and `span`; jewelry none.

| field | | meaning |
|---|---|---|
| `sort` | required | `weapon`, `armor`, `shield` or `jewelry` |
| `counts` | | bands of the item's own level, `[{ from, weights }]`: how often an item of the band has 0, 1, 2, ... enchantments |
| `damage_share` | | `[least, most]`, greatest damage as a share of a made weapon's at their level |
| `vigor` | | bands of the vigor they cost, `[{ from, least, most, every }]` |
| `span` | | `[least, most]`, a shield's span of damage when its template names none |
| `delays` | | `{ from, weights }`, how often each delay between blows from `from` up is drawn |
| `rating_share`, `rating_spread` | | a rating as a share of a made one's at their level, `[[level, share], ...]`, and how far either side of it each is drawn |
| `slot_chance` | | how often one has an enchantment slot |
| `prov` | required | where it came from |

#### `[[treasure_word]]`

A word before treasure items' names.

| field | | meaning |
|---|---|---|
| `sort` | required | `weapon`, `armor` or `shield` |
| `order` | required | its place in the order its sort's words are drawn in |
| `word` | required | its string in the install's table of these words (`0x2500035F`) |
| `delays` | | `[least, most]`, the delays between blows it goes with |
| `weights` | required | its weight in each band of its sort's `counts` |
| `prov` | required | where it came from |

#### `[[treasure_look]]`

The look a template's treasure items draw.

| field | | meaning |
|---|---|---|
| `template` | required | the template |
| `table` | required | the appearance table it is drawn in |
| `keys` | | one to three `{ key, most }`, each key and the highest value it draws, from 0 to 1 |
| `prov` | required | where it came from |

#### `[[treasure_potion]]`

A kind of treasure potion. A treasure potion draws its kind evenly and takes its dropper's tier.

| field | | meaning |
|---|---|---|
| `kind` | required | its name: lowercase letters, digits and hyphens |
| `order` | required | its place in the order the kinds are drawn in |
| `states` | | its effect's saved state at each tier from the lowest |
| `prov` | required | where it came from |

#### `[[treasure_family]]`

A family of enchantments one sort of treasure item draws from.

| field | | meaning |
|---|---|---|
| `sort` | required | `weapon`, `armor`, `shield` or `jewelry` |
| `order` | required | its place in the order its sort's families are drawn in |
| `when` | | `permanent` (on the item itself), `equipped` (on whoever wields or wears it) or `hit` (on what it hits) |
| `weight` | | how often it is drawn against its sort's others; 0: only as another's pair |
| `by_level` | | whether an item takes the family's one tier it reaches last, else any tier it reaches |
| `tiers` | | at least one tier, below |
| `prov` | required | where it came from |

| tier field | meaning |
|---|---|
| `least` | the least level of an item that reaches it |
| `state` | the enchantment, a saved state of the install |
| `puts_on` | what it puts on what the item hits, in a family of `hit` |
| `chance` | how often it does, from 0 to 1 |
| `glow` | the glow it gives weapons, from 0 to 1 |
| `pair` | an enchantment that comes with it in the same draw |
| `named` | `false`: never the one an item is named for |

A world with no rows of one of the treasure kinds plays the server's own of that kind. A kind's rows
are taken whole, never mixed with the server's.

### Crafting and gathering

#### `[[craft]]`

A craft skill or a recipe every new character starts with, at level 1 for a craft skill.

| field | | meaning |
|---|---|---|
| `skill`, `recipe` | one of them | the craft skill's saved state, or the recipe's |
| `prov` | required | where it came from |

#### `[[tool]]`

What a gathering tool (a knife or a mining tool) yields when it was not made.

| field | | meaning |
|---|---|---|
| `template` | required | the tool's template |
| `yield` | required | a count |
| `prov` | required | where it came from |

#### `[[butchery]]`

What butchering a corpse yields, by its creature's butchery profile. One material is cut per
butchery.

| field | | meaning |
|---|---|---|
| `profile` | required | the profile's saved state |
| `items`, `weights` | | the materials' templates, and each one's share, in order |
| `empty` | | the share of butcheries that find nothing, against the weights |
| `prov` | required | where it came from |

#### `[[mine]]`

What the nodes of a mining craft skill do. One row per skill.

| field | | meaning |
|---|---|---|
| `skill` | required | the craft skill (its saved state); a node mined by it is the row's |
| `behavior` | required | the behavior a miner plays with none of the skill's tools in hand |
| `tools` | | the skill's own tools |
| `answer`, `rich_answer` | required | string references: what a use answers, plain and when it finds twice the units |
| `delving` | | `{ effect, node }`: the effect a stone's use conjures the skill's node by, and the node |
| `lodestones` | | `{ from, template }`, levels rising from 0: the lodestone a find unearths |
| `find_chance` | | from 0 to 1: how often a use of a node a map led to is a find; 0 when left out |
| `finds` | | `{ template, share }`: the other items a find may be |
| `prov` | required | where it came from |

#### `[[map_region]]`

The nodes a region's surveyor's maps lead to.

| field | | meaning |
|---|---|---|
| `region` | required | a key: lowercase letters, digits and hyphens |
| `nodes` | required | at least one `{ effect, node }`: the name of an effect a map's use names, and the node it conjures |
| `prov` | required | where it came from |

An item whose template gives a rectangle to be used in and whose use names no surveyor's node (a
reckoning gem, a treasure map) needs no row to lead to its place: it leads to one in its rectangle
as a surveyor's map does, and only there does its use go on, its quest steps or its effects (what
`[[effect_rule]]` `places` and `[[death_spawn]]` rows say it makes among them).

#### `[[map_recipes]]`

A region's surveyor's map recipes, which wait together. One row per region.

| field | | meaning |
|---|---|---|
| `region` | required | a key: lowercase letters, digits and hyphens |
| `drawn` | required | the effect a drawing of one of the region's maps puts on its drawer |
| `found` | required | the name of the effect a find of one of the region's maps puts on |
| `recipes` | required | at least one recipe (its saved state) |
| `wait` | | seconds of game time; 300 when left out |
| `prov` | required | where it came from |

#### `[[spellbinding]]`

The totem spellbinding makes of a sort of item taken apart. One row per sort.

| field | | meaning |
|---|---|---|
| `sort` | required | `weapon`, `shield` or `armor` |
| `template` | required | the totem's |
| `word` | required | the string id its profile shows for what it goes on |
| `prov` | required | where it came from |

#### `[[refining]]`

What a refining recipe yields.

| field | | meaning |
|---|---|---|
| `recipe` | required | the name of the recipe's script package, such as `SteelRefiningRecipe` |
| `product` | required | the template of what it yields |
| `full_at` | | a craft skill level from 2 at which a refining yields all it takes; 150 when left out |
| `start` | | a percent from 0 to 100: what a refining yields of what it takes before any skill, growing evenly to all of it at `full_at`; 34 when left out |
| `prov` | required | where it came from |

#### `[[recipe]]`

A recipe the world defines where the install holds none, made from an install recipe: everything
the row doesn't set (its craft skill and category unless the row names them, its animation, checks
and cost) is that recipe's. Its outcomes make the row's product, stamp it with its maker and use up
each of its slots; whatever else the install recipe's outcomes did is left out. Auberean is sent
its record when it asks for one; a retail client, which never asks, is sent it as it logs in. The
install's own recipes need no patching, as every retail client holds them; a world with any
`[[recipe]]` row patches retail clients' data files, and they play it from a copy of them kept for
it. A step's `recipes` teaches it.

| field | | meaning |
|---|---|---|
| `recipe` | required | its id: a saved state id from `0x6F7E0000` to `0x6F7EFFFF`, the world's recipes' own, which no record of the install and no shop's copy (`[[store]]`) holds |
| `like` | required | the install recipe it is made from (its saved state) |
| `name` | required | a string reference: its name |
| `description` | | a string reference; none when left out |
| `ingredients` | required | its slots, in order, at least one: `{ items, count, name }`, the templates any of which goes in it, how many it takes (1 when left out), and a string reference for what the slot says it wants (none when left out) |
| `product` | required | the template of what it makes |
| `count` | | above 0; 1 when left out |
| `skill` | | its craft skill (a saved state); the install recipe's when left out |
| `category` | | its category in its craft skill, by number; the install recipe's when left out |
| `prov` | required | where it came from |

#### `[[combination]]`

What a recipe that works out its own product (a card meld) makes of one set of ingredients. A making
whose slots hold the row's templates, one a slot in any order, makes its product in the maker's pack
and uses up each slot as the recipe's outcome says; one whose slots make no row of its recipe makes
nothing, says "You fail to produce anything.", and uses up and pays nothing. The first row of the
recipe the slots make is taken.

| field | | meaning |
|---|---|---|
| `key` | required | lowercase letters, digits and hyphens |
| `recipe` | required | the install recipe (its saved state) whose product it works out |
| `items` | required | the templates its slots hold, at least one, one a slot in any order; a template twice where two of it go in |
| `product` | required | the template of what it makes |
| `count` | | above 0; 1 when left out |
| `prov` | required | where it came from |

#### `[[plant]]`

A species of plant a seed plants: a dye plant, or one of its own. One row per plant.

| field | | meaning |
|---|---|---|
| `plant` | required | the template of the plant a seed plants |
| `seed` | required | the template of its seed |
| `pollen` | | the template of the pollen a ripe plant gives; with `name` or not at all |
| `name` | | the pollen's name among the plants' texts (its string id) |
| `harvest` | required | the template of the plant item a ripe plant gives |
| `count` | | how many plants one seed plants, spread across the planter's heading; 1 when left out |
| `ripe` | | seconds from its planting until it is ripe, above 0; 180 when left out |
| `tended` | | false: it needs no water or fertilizer, never withers and keeps its own look until harvested; true when left out |
| `texts` | | `{ table, maturity_in, mature, harvest }`: the string table of its lines, holding the dye plants' lines for a plant not yet ripe and one its user did not plant under their ids, and the ids of its examination's countdown label, its line once ripe and its harvest's line; the dye plants' when left out |
| `prov` | required | where it came from |

#### `[[plant_cross]]`

What a ripe plant pollinated with a pollen gives in place of its plant item and pollen.

| field | | meaning |
|---|---|---|
| `plant`, `pollen` | required | the plant's template and the pollen's; one row per pair |
| `seeds` | required | the template of the seeds it gives |
| `count` | required | 1, 2 or 3 |
| `prov` | required | where it came from |

### Creatures and pets

#### `[[weapon]]`

The weapon a creature wields, where the name its templates give it is no template of the install's
own.

| field | | meaning |
|---|---|---|
| `name` | required | the name, as creatures' templates write it |
| `template` | required | the template it is made of |
| `two_share` | | of the creatures whose template asks two of it, the share given two |
| `stance` | | the stance it is fought with, where its template sets none |
| `prov` | required | where it came from |

#### `[[death_spawn]]`

What a creature's death effect makes around its killer, or an item's use effect around its user,
where the install names it by a name it doesn't resolve: creatures, or a portal.

| field | | meaning |
|---|---|---|
| `effect` | required | the effect, as templates name it |
| `template` | required | what it makes: a creature, or a portal whose template names a destination |
| `count` | | how many it tries to make; 1 when left out |
| `chance` | | that each is made; 1 when left out |
| `offset` | required | where each stands, in meters from the killer or user as it faces, `[x, y, z]` |
| `lasts` | | how long each stands, in seconds; left out, a creature until killed, a portal until the server stops |
| `kingdom` | | a portal's: the kingdom a player must be of to use it, 1 (Shadow), 2 (Order) or 4 (Dominion) |
| `prov` | required | where it came from |

A portal leads where its template's destination names: the `[[place]]` whose `named_by` holds the
template, else the one whose `destination` is the template's destination's name. With neither, the
row makes nothing and the item whose use names it is kept. Anyone may use the portal while it
stands, as a placed portal of its template: its least level, its heroes-only flag, the quest it
needs and its use effects are the template's, and its portal flags say whether its user may recall
to where it leads.

#### `[[pet]]`

What a pet effect makes, or what a contract hires. One row for each effect.

| field | | meaning |
|---|---|---|
| `effect` | required | the pet effect's saved state, or the contract's template, or for a pet only items make the item's template, or for a step's `escort` its template |
| `template` | required | what it makes |
| `flags` | | the flags of its pet data as its owner is told them; 0 when left out |
| `count` | | how many the effect makes at once, at least 1; 1 when left out |
| `stays` | | true: it stays where it is made and never follows |
| `health` | | its greatest health at levels, `{ level, health }`, levels going up; its template's when left out |
| `armor` | | its armor against a creature's hit at levels, `{ level, armor }`, levels going up |
| `per_level_squared`, `variance`, `lands` | | what a hit of it takes: `per_level_squared` times its level squared, varied by `variance` either side; `lands` is the share of its strikes that land, from 0 to 1 |
| `swing` | | hits a swing on its one target, at least 1 |
| `area` | | its missile lands on every creature this many meters from where it falls |
| `taunts` | | true: its strikes taunt |
| `ends_on_strike` | | true: it ends as its first strike lands |
| `vigor_per_level`, `vigor_per_second` | | its greatest vigor for each level; it pays its attacks' vigor, which comes back this much a second |
| `outdoors_only` | | true: it can't be set down indoors |
| `attacks` | | the attacks it uses, `{ skill, weight }`, each skill one its template names |
| `corpse_after`, `no_corpse` | | seconds after its death its corpse comes (1.5 when left out); true: it leaves no corpse |
| `tends` | | whom it tends: `{ whom = "owner" or "owners-pets", effect, every, below }` |
| `hire` | | a contract's terms: `{ class, seconds, level }` |
| `item_effects` | | names of effects an item's use may name that no install holds, each making this pet |
| `prov` | required | where it came from |

#### `[[pet_limit]]`

How many pets a summoning skill allows its user.

| field | | meaning |
|---|---|---|
| `skill` | required | the skill's id |
| `limits` | required | at least one `{ most, classes }`: a use is refused while its user has `most` pets of a class sharing a bit with `classes` |
| `prov` | required | where it came from |

#### `[[concealing]]`

What creatures make of the holder of a concealing effect. One row per effect and class.

| field | | meaning |
|---|---|---|
| `effect` | required | the effect, one the install holds |
| `class` | | the creatures' class of behavior (their `AI_AISuperClass`); left out, every creature |
| `regard` | required | `spares` (they come at the holder only once it hurts them) or `hates` (they come at it) |
| `prov` | required | where it came from |

#### `[[npc_table]]`

What the world says of the creatures of a behavior table.

| field | | meaning |
|---|---|---|
| `name` | required | the install's table, their `AI_NPCTable` |
| `heals_kin` | required | true: a healer of the table heals its badly hurt kin as well as itself |
| `prov` | required | where it came from |

#### `[[friend_table]]`

What the world says of the creatures of a friend table. A row gives at least one of
`attacks_first` and `fights`.

| field | | meaning |
|---|---|---|
| `table` | required | its number, their `AI_LoveTable` |
| `attacks_first` | | false: they never attack a player first |
| `fights` | | the friend tables (numbers) whose creatures they take on, the nearest within their perception, nearer than any player they would, while a player is near; they turn from one only to a player that hurts them |
| `prov` | required | where it came from |

#### `[[creature]]`

What the world says of the creatures of a template: its shooting rhythm, the mark it puts on the
players it sees, whether it takes on only the players it hates, and whether players may harm it. A
row gives at least one of these.

| field | | meaning |
|---|---|---|
| `template` | required | the template |
| `shot_gap`, `windup` | | the gap between its shots, and the time from the start of a shot until the missile leaves, in seconds above 0, the two together |
| `marks` | | an effect (its saved state) it puts on each player that comes within its perception, once each time the player comes in |
| `mark_lines` | | with `marks`: the line it says aloud as it marks, naming the player, `{ race, line }`, one to a race (no race: any other), `line` a string reference `{ table, string }` |
| `only_hated` | | true: it takes on only a player a concealing effect makes it hate, and gives up on one that no longer holds the effect |
| `players_kept_out` | | true: players and their pets may not harm it; an attack on it is refused |
| `kept_out_line` | | with `players_kept_out`: the line a player's attack on it is told, a string reference `{ table, string }` |
| `prov` | required | where it came from |

A world with no rows of `[[concealing]]`, `[[npc_table]]`, `[[friend_table]]` or `[[creature]]`
has the server's own of that kind.

### Effects, events and items

#### `[[effect_name]]`

The saved state an effect's name stands for, where the install names the effect and the server
can't resolve the name.

| field | | meaning |
|---|---|---|
| `name` | required | the name, as text |
| `state` | required | the saved state, `0x6F...` or `0x70...` |
| `prov` | required | where it came from |

#### `[[effect_rule]]`

What an effect of the install does where its own data says nothing. One row per effect.

| field | | meaning |
|---|---|---|
| `effect` | required | its saved state, one the install holds; or, for an effect the install's templates name (an item's use) but holds no saved state of, its name as text, such a rule naming only `places` |
| `ward` | | `{ classes, effects, chance }`: what a player holding it keeps off |
| `vitae_share` | | the share of its player's vitae it takes when it is put on, above 0 and at most 1 |
| `vitae_restore` | | `{ points, fellows }`, the vitae it gives back when it is put on |
| `then` | | the effect is an immunity under way and leaves this effect as it ends |
| `reset` | | `skills` or `hero-skills`: putting it on grants its player one complete reset of that kind, which `/skills reset` or `/heroskills reset` uses |
| `skill` | | a skill, by its number: putting it on trains its player that skill for good, at level 1 by the grant, unraisable and at no cost, told with the effect's own line and appearance; a player who holds it gains nothing |
| `gives` | | a template: putting it on makes items of it in its player's pack, as the server's gift, with no line of their own; for an effect that names no item it gives, such as one that hands over a quest's item |
| `count` | | with `gives`: how many, above 0; 1 when left out |
| `places` | | a template: putting the effect on (by name: using an item whose template names it, after the use is answered) makes an object of it standing in the world at its player, no one's to take or use; one of a chest's template is a chest anyone may open, filled from its generator profile at each opening (a treasure map's cargo chest), and what it holds goes with it |
| `offset` | | with `places`: `[across, ahead]`, meters from the player as it faces, on the ground; `[0, 0]` when left out |
| `lasts` | | with `places`: seconds it stands, above 0; left out, until a `[[tally]]` that `clears` it resets, or the server stops (what is placed is not kept across a restart) |
| `prov` | required | where it came from |

#### `[[points_bonus]]`

What an effect does to the kingdom points of the player it is put on, where the install gives no
amount. One row per effect.

| field | | meaning |
|---|---|---|
| `effect` | required | its saved state, one the install holds |
| `add` | required | above 0 |
| `up_to` | required | kingdom points: the points rise by `add`, never above this |
| `prov` | required | where it came from |

#### `[[popup]]`

A tip a player is shown once, the first time a moment comes for its character. One row per moment.

| field | | meaning |
|---|---|---|
| `on` | required | the moment: `first-entry`, `level-up` or `glyph` |
| `effect` | required | the saved state of an effect whose description is the tip |
| `prov` | required | where it came from |

#### `[[event]]`

A named state of the world, on for its times, and while a full tally starts it. A camp's or an
object's `event`, and a step's `requires = [{ event = "key" }]`, name one.

| field | | meaning |
|---|---|---|
| `key` | required | lowercase letters, digits and hyphens |
| `game_event` | | a value of the install's game events it stands for |
| `when` | | at least one window: `{ from, until }` dated (UTC times `YYYY-MM-DDTHH:MMZ`), or `{ days, from, until }` weekly (`days` from `mon` ... `sun`, times `HH:MM`, UTC); left out, it is on only while a tally's `starts` names it, and one must |
| `on_players` | | effects every player in the world holds while it is on |
| `prov` | required | where it came from |

#### `[[tally]]`

A count the whole world keeps, the same for every player: steps and gives `adds` to it as they
run, never past its `full`; steps, gives and lines may `require` it full or not yet full; and a
line may say it (`fills`). The count, and when it filled, are kept across a restart.

| field | | meaning |
|---|---|---|
| `key` | required | lowercase letters, digits and hyphens |
| `full` | required | the count at which it is full, above 0 |
| `starts` | | `{ event, after, lasts }`: as it fills, the `[[event]]` comes on `after` seconds later (0 when left out) for `lasts` seconds (above 0) |
| `resets` | | when it goes back to 0: `{ event = "key" }`, as that event ends (also when it ended while the server was down); `{ every = S }`, every S seconds by the calendar, counted from 00:00 UTC on 1 January 1970 (86400: each day at 00:00 UTC); left out, never |
| `clears` | | templates: as it resets, every object of them standing in the world that the world made while it ran (what effects placed, items left on the ground; no creature, nothing held) leaves it |
| `prov` | required | where it came from |

#### `[[item_use]]`

What an item's use does where its template names no effect for it. One row per template.

| field | | meaning |
|---|---|---|
| `template` | required | the item's template |
| `effects` | | `[{ effect, spellcraft }]`, at least one, put on its user in order |
| `behavior` | | what its use plays where its template names none |
| `wears` | | above 0: it wears by its uses, down one a use, gone at the last |
| `prov` | required | where it came from |

#### `[[book]]`

What an item or a person of a template shows when used, where its template names nothing.

| field | | meaning |
|---|---|---|
| `template` | required | the template |
| `table`, `picture` | one of them | the string table of a book's pages, or a picture shown with no page controls |
| `prov` | required | where it came from |

#### `[[start_item]]`

What every new character is given as it first enters, beside its race's starting pack.

| field | | meaning |
|---|---|---|
| `template` | required | the item's template |
| `count` | | above 0; 1 when left out |
| `prov` | required | where it came from |

#### `[[interaction]]`

What an item used on another makes, where the item's interaction table lets it be used so and the
table's outcome names nothing made: an emblem used on a weapon it is imprinted from. The user is given
it after the quest steps the use triggers and before the item is used up. A use takes the row for the
item that names the target's template, else the first that names a class the target counts in.

An item's table may hold interactions for some characters only: those whose number (the low word of
the character's id) leaves a given remainder when divided by a given number. A use takes the table's
first interaction naming the target that its user's character may have, so a character always meets
the same one, and characters spread across them by their numbers. Where the interaction met is one of
these, the row gives its `gated_gives`, if it names one. A card meld's table names each lesser trump
(Ace 0 to King 13) for the characters whose number leaves its rank when divided by 14, then each suit
for anyone: one row a suit, `gives` for anyone and `gated_gives` for a character on its own rank,
covers the suit.

A row may instead, or as well, name what the use makes standing in the world at the object it was
used on (`places`): a recondite machine used on a reckoning gem's dig point uncovers a cache and the
guards it wakes. Each stands at its offset from the object as the user faces, on the ground: a chest
that anyone may open, filled from its generator profile; another object, no one's; a creature,
until it is killed. Where the item's table plays a behavior of its own the use plays it first; where
the table's outcome uses up the object, an object an effect placed (the dig point) leaves the world.

| field | | meaning |
|---|---|---|
| `key` | required | lowercase letters, digits and hyphens |
| `item` | required | the template of the item used |
| `on`, `on_class` | one of them | what it is used on: a template, or a class (a script package, by number) the item's table names, any template of which counts |
| `gives`, `gives_one_of` | one of them, or `places` | the template of what it makes, or one of several picked by weight each use: at least two `{ template, weight }`, each weight above 0 (a meld making one of a fortune card's two sets: `[{ template = A, weight = 1 }, { template = B, weight = 1 }]`) |
| `gated_gives`, `gated_gives_one_of` | | what it makes instead where the interaction the use met is for some characters only, one template or one of several as `gives_one_of`; `gives` when left out |
| `count` | | above 0; 1 when left out |
| `places` | | at least one `{ template, offset, lasts }`: what it makes at the object, `offset` `[across, ahead]` in meters as the user faces (`[0, 0]` when left out), `lasts` seconds above 0 (left out, until it is killed, cleared or the server stops) |
| `prov` | required | where it came from |

#### `[[object_states]]`

The states the world's objects of some templates hold: each such object is in one of them, at the
start its template's first. An item given to one, where a change names the item and the state the
object is in, puts it in the change's state and is handed back, nothing used up; an item no change
names is answered as any other give. The object goes back to its first state `reverts` seconds after
its last change. The state is the object's, the same for every player. A template holds the states
of one row at most.

| field | | meaning |
|---|---|---|
| `key` | required | lowercase letters, digits and hyphens |
| `templates` | required | `{ template, start }`: a template whose objects hold a state, and the state they start in |
| `states` | required | the states' names (lowercase letters, digits and hyphens), two or more, each once |
| `reverts` | | seconds after its last change an object goes back to its first state, above 0; never when left out |
| `changes` | | `{ item, from, to }`: an item given to an object in state `from` puts it in state `to`; an item and a state once |
| `prov` | required | where it came from |

#### `[[sequence]]`

Uses in an order, judged against the used objects' states, and the quest whose phase counts the right
uses a player has made: a use of the next in the state named moves the quest on a phase, the first
giving it, with what its definition names for that update, and the last takes the player to `to`; a use of any other object the row names, or
of the next in another state, takes the quest away from a player who has it under way, and the order
starts over. Every row naming the object used is judged at each use, so one use may move several on.
A row whose quest is past its last use, or done and not to be given again, is not judged. The object
says the `right` line of the first row a use moved on, else the `wrong` line of the first it judged; a
use of an object a row names runs no quest step of its own.

| field | | meaning |
|---|---|---|
| `key` | required | lowercase letters, digits and hyphens |
| `quest` | required | the quest whose phase counts the right uses |
| `uses` | required | `{ use, state }` in order: the template used and the state its object must be in (`[[object_states]]`, loaded before; any state when left out) |
| `right`, `wrong` | | what the object used says at a right use and at a wrong one, `{ table, string }` |
| `to` | | the place the last right use takes the player to, through portal space (`key`, or `pack:key`), in place of a recall the quest's definition names for that update; without it, such a recall takes the player |
| `prov` | required | where it came from |

#### `[[lever]]`

Placed objects whose use opens doors of the world for everyone. A player who uses a lever is walked
to within three meters of it and pulls it, playing a key's turning, while the lever plays its
template's use animation. Once every lever of the row is pulled (a single lever at each pull; several
each within `within` seconds of the last, when the row names it), the row's doors open as a use
opens a door, whatever their locks, and stay open `open_for` seconds in place of a door's own ten,
then close by themselves; with no `open_for` they stay open until a use closes them. A pull while
they stand open holds them so again from then. A door `locked` with no `lock` is one no key opens:
from its front only a lever opens it, from behind anyone may. A lever may be in several rows, and
one pull counts in each.

| field | | meaning |
|---|---|---|
| `key` | required | lowercase letters, digits and hyphens |
| `levers` | required | the placed objects pulled, at least one: `key@XXYY`, or `pack:key@XXYY` in another pack |
| `opens` | required | the doors their pulls open, at least one, named as `levers` |
| `open_for` | | how long the doors stay open, in seconds, above 0; until a use closes them when left out |
| `within` | | how long a pull waits for the row's other levers, in seconds, above 0; for as long as it takes when left out |
| `prov` | required | where it came from |

#### `[[offering]]`

A placed object that takes a set of items from players and then opens the way for a while. An item
of the set given to it, while it does not hold that item and is not open, is kept (it leaves the
giver's pack and the world), whoever gives it: what it holds is the world's, the same for every
player, and it says its lines for a give of that item. A part it holds already, or any while it is
open, is handed back as a give it wants nothing of. An item of the set may name a placed object
that stands in the world only while the offering holds that item. As the last item comes, those go
and the offering's `opens` stand for everyone for `lasts` seconds; then they go and it holds
nothing again. What it holds and how long it stays open last over a restart. A quest step the give
triggers runs instead; an item not of the set is the object's as any other give. An object takes
one offering.

| field | | meaning |
|---|---|---|
| `key` | required | lowercase letters, digits and hyphens |
| `object` | required | the placed object given to: `key@XXYY`, or `pack:key@XXYY` in another pack |
| `takes` | required | `{ item, shows }`, at least one, each item once: an item of the set, and the placed object that stands while it is held (named as `object`; none when left out) |
| `opens` | required | the placed objects that stand once it holds the whole set, at least one, named as `object` |
| `lasts` | required | how long they stand, in seconds, above 0 |
| `prov` | required | where it came from |

#### `[[worn]]`

A look an item of a template gives its wearer, where the template's data names none. A wearer
takes the first of its template's rows that fits it.

| field | | meaning |
|---|---|---|
| `template` | required | the item's template |
| `races` | required | the races it fits, a mask (1 human, 2 lugian, 4 tumerok; `0xFFFFFFFF` every race) |
| `sexes` | required | the sexes it fits, a mask (`0x1000` male, `0x2000` female; `0xFFFFFFFF` both) |
| `appearance` | required | the appearance it gives them |
| `prov` | required | where it came from |

A world with no rows of `[[item_use]]`, `[[book]]` or `[[start_item]]` has the server's own of that
kind, and so does one with no `[[worn]]`, `[[mine]]`, `[[map_region]]`, `[[map_recipes]]`,
`[[spellbinding]]`, `[[refining]]`, `[[plant]]` or `[[plant_cross]]` rows; a world with any has its
rows alone.

### Taking rows away

#### `[[remove]]`

The items a pack takes away, each named as the table under Keys and references names it.

| field | | meaning |
|---|---|---|
| `items`, `item` | one of them | the items: `items = ["camp c-4586@77B8"]`, or `item = "camp c-4586@77B8"` |
| `prov` | | where it came from |

## What a row never holds

- the game's own text: names, descriptions, lines, journals (referenced by string id instead);
- bytes of the client's records, or values a build reads from the install;
- names of players or characters, chat, or anything identifying a recording or the person who
  made it;
- anything from other projects' databases.
