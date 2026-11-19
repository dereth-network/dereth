# Auberean plugins

**Provenance: design.** The plugin interface is this project's own addition, not the original game's; every statement here is what an Auberean host must do.

Auberean, the Asheron's Call 2 client, loads plugins written against the same interface as
Dereth's: the same packages and manifests ([manifest.md](manifest.md)), the same sandbox, budgets
and faults ([guide/lifecycle-and-sandbox.md](guide/lifecycle-and-sandbox.md)), the same events,
tasks, commands, stores, declared windows and services. What a plugin hears of the game and asks of
it is Auberean's own, and is described here. It covers what the game tells (its events, and what the
player's scripts tell the game's panels), what a plugin reads of it (the player, the objects around,
the selection, the inventory, the effects, the skills, the questions, where the player stands, the
fellowship, allegiance and friends, the journal, the map's markers, the pets, the shortcut bar,
crafting), and what a player asks of it through the chat, the selection, combat, the controls, the
camera, offers and trades, the fellowship, allegiance and friends, the pets, the shortcut bar and
crafting.

## A plugin for Auberean

A manifest names the game:

```toml
id = "example.effects"
name = "Effects"
version = "1.0.0"
api = "0.7"
game = "auberean"
contexts = ["pregame", "world"]
```

An Auberean host refuses a plugin for Dereth (a manifest naming no game is Dereth's), and Dereth's
hosts refuse one for Auberean, telling the player why: "it is a plugin for Dereth, not Auberean".

A plugin is active in `world` (a character is in the world) or `pregame` (the character list and
entering the world), as its `contexts` say; `world` when it names none. Auberean has no separate
`chargen` part: making a character happens at the character list, which is `pregame`.

**Ids are text.** An object of the game is named by its id as text, `0x` and sixteen lower-case hex
digits (`"0x2130000000000001"`): the game's ids are 64 bits wide, more than a Lua number holds
exactly. Every event gives ids this way, and every function that takes one accepts it (with or
without the `0x`).

## What every game shares

These work as in Dereth ([api.md](api.md)): `on` and `off`; `print` and `chat.print`; `chat.say`
and `chat.command` (said or carried out as the player's typed line); `commands.add` and
`commands.remove`; `task.every`, `task.after`, `task.spawn`, `task.sleep` and `task.cancel`;
promises, `await` and `pawait`; `store.global`, `store.world` and `store.character`; `files.package`
and the data folder; `ui.window`, `ui.hud`, a window's handles, `ui.toast` and `ui.status`;
`services`; `permissions`; `host`; `os`; `require`; `session.info`, `session.select_character` and
`session.logout`.

Not yet in Auberean's hosts, each failing with a message saying so: `chat.hide`, `peers.send`,
`net.fetch`, `files.import` and `session.quit`. No Auberean world publishes a plugin policy, so
`policy.info()` says none is published and every action is allowed.

A window or HUD a plugin declares opens when the player opens it, or when the plugin shows it while
answering the player (one of its commands typed, a press on one of its windows).

## Events

Each frame a plugin hears, in this order: its contexts ending and beginning; then the game's events,
in the order the client told them; then the actions that ended, the player's commands and presses
on its windows, the service calls, and the timers that came due.

The events every game shares that an Auberean host delivers:

| event | the table's fields |
|---|---|
| `session.pregame` | `characters` (a list of `{ id, name }`), `world`, `account`: the account's characters are listed |
| `session.login` | `id`, `character`: entering the world, as the character (when the host knows which) |
| `session.login_failed` | `code` (the server's reason), `reason` (`character_in_world`, `booted` or `refused`) |
| `session.enter_world` | `id`: the character is in the world |
| `session.logout` | none: back at the character list from the world |
| `session.disconnect` | none: the connection ended |
| `chat.line` | `text`, `kind` (the kind of text, a number; each chat window shows some kinds), `about` (the object it is about, or nil) |
| `ui.clicked`, `ui.opened`, `ui.closed` | `window`, `id` (a press's) |
| `action.completed`, `action.failed` | as in Dereth ([events.md](events.md)) |
| `service.call` | as in Dereth ([services.md](services.md)) |
| `plugin.loaded`, `context.begin`, `context.end` | as in Dereth |

Auberean's own:

| event | the table's fields |
|---|---|
| `chat.said` | `speaker` (nil when the server named no one), `text`: someone nearby said something |
| `chat.told` | `teller`, `name`, `text`: a tell to the player |
| `social.recruitment_offered` | `leader`, `leader_name`, `fellowship`: a place in a fellowship is offered |
| `social.oath_offered` | `vassal`, `name`: another player offers their oath |
| `trade.registered` | `asker`, `asked`, `partner` (the other side's name): a trade was set up |
| `trade.opened` | none: the trade window opened on both sides |
| `trade.accepted` | `by`: one side accepted the trade as it stands |
| `effects.added` | `effect`: an effect was put on the player, by its number |
| `effects.removed` | `effect`: an effect was taken off the player |
| `craft.done` | `recipe`, `error` (0: it went), `shown`: the server answered a making |

### What the panels hear

What the player's scripts tell the game's panels reaches a plugin as one event for each part of the
game, after the frame's other events and in the order told. Each event's table has `about` (the
object the notice is about, nil when it is for every panel), `what` (a word saying what it says) and
that word's fields. A name the game gives only as a string reference comes as `<field>_ref`, as the
reads give it (below). A notice the client does not read is not told.

| event | `what`, and its fields |
|---|---|
| `notice.player` | `vital` (`vital`: `health`, `vigor`, `experience`, `focus`, `health_taken` or `vigor_taken`; `current`, `max`), `hero` (`hero`, `focus`, `max_focus`), `vitae` (`kept`, `regained`, `experience`), `conflict` (`conflict`, as `game.social` names it), `death` (`dead`), `mounted` (`mounted`), `effects_changed`, `pulse_missed` (`effect`), `options_read` and `options_changed` (`options`, the yes-or-no options' bits, when known) |
| `notice.selection` | `changed` (`selected`, nil for nothing), `selectable` (`selectable`), `vitals` (`kind`, `health`, `max_health`, `health_taken`, `vigor`, `max_vigor`, `vigor_taken`), `targeting` (`waiting`: an item waits for its target) |
| `notice.examination` | `profile` (`request`, `tabs`, `lines`), `request_changed` (`request`), `open`; a request is `kind`, `object`, `tabs`, `data_enum`, `data_id`; a line `kind`, `tabs`, `appearance`, `value`, `time`, `text`, `icon` (a list of `{ picture, color }`) and an effect's own lines (`effect`) |
| `notice.chat` | `start_tell` (`to`), `settings_read` |
| `notice.dialog` | `message` (`text`, `at_once`), `resurrection_offered` (`by`), `effect_question` (`target`, `source`, `effect`, `question`) |
| `notice.world` | `look_changed`, `item_changed`, `damage` (`text`, `color` as red, green, blue and alpha, `seconds`, `scale`) |
| `notice.combat` | `attacking` (`attacking`), `special_asked` (`skill`, `special`), `special_started` and `special_ended` (`special`), `special_toggled` (`skill`, `on`) |
| `notice.skills` | `changed` (`skill`), `points` (`experience`, `credits`, `hero_credits`, `whole`), `reset_times` |
| `notice.shortcuts` | `reloaded`, `recipe_gone` (`recipe`) |
| `notice.inventory` | `worn` (`change`, `item`, `location`), `contents` (`container`, `change`, `item`, `slot`), `container_state` (`container`, `open`, `state`), `pack_changed`, `money` (`amount`); a `change` is `all`, `added`, `removed`, `failed` or `under_way` |
| `notice.fellowship` | `changed` (`fellowship`, as `game.social` gives it), `cleared`, `fellow` (`fellow`, `record`), `fellow_gone` (`fellow`), `level` (`fellow`, `level`), `health` and `vigor` (`fellow`, the vital, `taken`), `max_health` and `max_vigor` (`fellow`, `max`), `leader` (`leader`), `cell` (`fellow`, `cell`), `offered` (`leader`, `leader_name`, `fellowship`), `offer_expired` (`leader`) |
| `notice.allegiance` | `changed` (`allegiance`, as `game.social` gives it), `oath_offered` (`vassal`, `name`), `oath_answered` (`patron`, `answer`) |
| `notice.friends` | `list` (`friends`), `added` and `removed` (`name`), `state` (`friend`), `own_state` (`mode`) |
| `notice.trade` | `registered` (`asker`, `asked`, `partner`), `opened`, `closed` (`by`, `reason`), `accepted` and `declined` (`by`), `emptied`, `acceptance_cleared`, `offered` (`by`, `item`, `quantity`), `withdrawn` (`by`, `item`) |
| `notice.pet` | `added` (`pet`, `name`, `icon`, `mode`), `removed` and `attacked` (`pet`), `named` (`pet`, `name`), `mode` (`pet`, `mode`) |
| `notice.map` | `marker_added` (`kind`, `object`, `cell`, `picture`, `label`), `marker_removed` (`kind`, `object`), `markers_cleared` (`kind`) |
| `notice.store` | `catalog` (`header`, `shops`), `catalog_left`, `shown` and `changed` (`store`), `stock` (`item`, `stock`), `left`, `purchase` (`product`, `quantity`, `cost`, `cost_quantity`), `consignments` and `consignments_changed` (`store`), `consignments_left`, `sale_waiting` (`waiting`); a store is `id`, `name`, `description`, `sorter`, `portrait`, `position`, `size` and its `sales` (each `item`, `item_data`, `icon`, `description`, `cost_description`, `cost_icon`, `cost_quantity`, `stock`, `ordinal`, `state`) |
| `notice.craft` | `panel_changed`, `ingredient_added` (`recipe`, `slot`, `item`, `amount`, `error`), `made` (`recipe`, `error`), `open` (`recipe`), `skill_changed` (`skill`) |
| `notice.quest` | `journal` (`change`, `quest`, and `all` when the whole list was read again; each quest as `game.journal` gives it), `acts` (`change`, `act`), `scenes` (`scenes`: each `id`, `playable` and its `keyframes`, each `picture`, `music`, `voice`, `subtitle`, `seconds`) |
| `notice.book` | `open` (`controls`, `look`, `title`, `text`, `picture`) |
| `notice.interface` | `through_portal` (`inside`), `portal_scene` (`playing`, `scenes`), `save_layout` and `load_layout` (`name`), `chat_service_lost`, `chat_service_ticket` (`taken`) |

## Reading the game

Each of these answers from the game's view as it is now: the one the game's interfaces read, as
the host last brought it up to date (once a frame). A host with no view fails them, saying so.

| function | what it answers |
|---|---|
| `game.player()` | `id`, `name`, `world`, `race`, `health`, `vigor`, `max_health`, `max_vigor`, `place` (`cell`, `x`, `y`, `z`, `heading`), whether it is `moving` and whether it is `mounted`; nil before the player's object is here |
| `game.objects()` | the objects around the player, each as `game.object` gives it |
| `game.object(id)` | `id`, `name`, `kind`, `data_id`, `place` (nil when it has none), the `parent` it hangs from, its `owner` and `container`, and whether it is `selectable`, `dead`, a `fellow` and `attackable`; nil when it is not around |
| `game.selection()` | `current` and `last` (nil for nothing), and `auto_target` |
| `game.inventory()` | `money`, and `items`, each with its `id`, `name`, `quantity`, whether it is `worn` and `stackable` |
| `game.effects()` | the effects on the player: each one's `id`, `name` and `remaining` seconds (nil when it does not run out) |
| `game.skills()` | the player's skills: each one's `id`, whether it is `trained`, its `level` and `base_level` |
| `game.clock()` | `world_time` and `game_time`, seconds |
| `game.questions()` | the game's questions open, in the order asked: each one's `id`, `kind` (`died`, `resurrection`, `effect`, `message`, `fellowship`, `oath` or `trade`), `text`, whether it has `one_answer` and goes in front (`at_once`), and who it names (`by`, `leader` and `leader_name` with the `fellowship`, `vassal` or `partner` with their `name`) |
| `game.place()` | where the player stands, as the radar's place line gives it: whether it is `inside`, its map `coordinates` outdoors (`east`, `north`; nil inside), whether it is in `portal_space`, and how far the radar reaches there (`radar_range`, meters); nil before the player is in the world |
| `game.social()` | `fellowship` (nil while in none: its `name`, `leader`, whether it shares experience, `share_experience`, its `looting`, and its `fellows` in the order they joined, each `id`, `name`, `level`, `health`, `max_health`, `vigor`, `max_vigor`, what players took of each, `health_taken` and `vigor_taken`, and `cell`); `allegiance` (nil until told: its `name`, `message`, `faction`, `total`, `officers`, and `monarch`, `patron`, the player as `member` and `vassals`, each `id`, `name`, `title`, `online`, `level`, `rank`, `officer`, `faction`, `species`, `gender`, `experience_passed`); `friends` (each `name`, `location` and `mode`: 0 offline, 1 available, 2 away, 4 busy); the player's own chat `state`; its `kingdom` (`shadow`, `order`, `dominion` or `neutral`; nil for none); the `conflict` the place allows (`peace`, `controlled`, `kingdom_against_kingdom`, `free_for_all` or `kingdom_against_kingdom_anywhere`) |
| `game.journal()` | the player's quests, newest given first: each one's `id`, `name`, `description`, `journal`, `status`, `challenge` (-999: any level), when it was `given` (game seconds), whether it is `maxed_out`, its `seconds_remaining` and whether it is `cancellable`; nil before the player is here |
| `game.markers()` | the markers on the map: each one's `kind`, the `object` it stands for (nil for none), the `cell` it marks, its `picture` and its `label` |
| `game.pets()` | the player's pets in the order they came: each one's `id`, `name` (as the game shows it, without its owner's part), `icon` and `mode` (`aggressive`, `opportunistic`, `courageous`, `protective`, `assisting`, `defensive`, `supporting`, `cooperative` or `idle`) |
| `game.toolbar()` | the shortcut bar: the `set` of ten shown (from 1), whether it is `locked`, shows its `numbers`, its `rows`, and the `slots` that hold something, each with its `slot` number (0 to 99), its `kind` (`skill`, `item`, `text`, `emote` or `recipe`), what the kind names (`skill`, `item`, `text`, `emote` or `recipe`), its `tooltip` and its `icon`; nil until the game answers for it |
| `game.craft()` | the crafting page: the player's craft `experience`; the craft `skills`, each `id`, `name`, `level`, whether it is `trained`, whether it `can_raise`, and its `recipes` (each `id`, `name`, `category`, whether it is `shown`, and the game time it may be made again, `use_again`); the recipes' `categories` (each `id` and `name`); the recipe the making window has `opened` (its `recipe`, `name`, `spinner`, `min_spinner`, `max_spinner`, `charges`, `difficulty`, `cost`, its `ingredients`, each `slot` and `description`, and the `items` that may go in), nil when none is; nil until the game answers for it |

A name the game gives only as a string of its string tables, before those have arrived, is left out
and named instead as `name_ref` (`{ string, table }`).

## Asking the game

Each of these is carried out as the frame ends, as the game's own interface would for the same
gesture; it answers nothing, and what follows arrives as events.

| function | what it asks |
|---|---|
| `act.select(id)` | select an object (nil: nothing), as a click on it in the world does |
| `act.select_next(kind, previous)` | select the next object of a kind around the player (`previous`: the one before), as the selection keys do; the kinds are `creature`, `item`, `player`, `foe`, `fellow` and `pet` |
| `act.select_nearest(kind)` | select the nearest object of a kind around the player |
| `act.select_self()` | select the player (when it is selected already, the selection before) |
| `act.select_last()` | select the selection before this one |
| `act.select_last_attacker()` | select the last to attack the player |
| `act.look_at(id)` | turn the player's head toward an object (nil: straight ahead) |
| `act.use_selected()` | use what is selected, as a double click or the use key does |
| `act.use_slot(n)` | use what shortcut slot `n` holds |
| `act.attack(start)` | start attacking what is selected (`false`: stop) |
| `act.cease_attack(id)` | stop attacking an object |
| `act.show_names(on)` | show or hide the names over heads |
| `act.control(id, extent)` | press a control of the game's action map (by its id) as a key bound to it would; `false` or `0` lets it go, a number is the press's extent |
| `act.answer(id, yes)` | answer one of the game's open questions (`false`: no), as its box in the game's interface would; one with one answer takes either |
| `act.release_controls(typing)` | let go of every control in force (with `typing`, the modifier and interface controls stay) |
| `act.camera(settings)` | choose the orbit camera, with its `movement` (`camera`: the controls run where the camera looks; `character`: the game's own steering), `reverse_x`, `reverse_y`, `mouse_turn` and `key_turn` (each left out as the default, the rates held to their ranges); nil: the game's own camera |
| `chat.window(n, settings)` | set chat window `n`'s `opacity` (0 to 1), `filter` (the kinds of text it shows, a mask) or `devote` (the kind it is given over to) |
| `chat.kind(kind, settings)` | set a kind of text's `size`, `color` or `popup` (whether it pops the chat up) |
| `chat.keep()` | send the chat windows' settings to the server to keep |
| `social.answer_fellowship(accept)` | take the place offered in a fellowship (`false`: turn it down) |
| `social.answer_oath(accept)` | take the oath offered (`false`: refuse it) |
| `trade.ask(what, partner)` | `"open"` (with the partner who asked), `"accept"`, `"decline"` or `"close"` |
| `social.create_fellowship(name, share_experience, looting)` | make a fellowship (sharing experience unless `false`; `looting` as the game numbers it, 0 when left out) |
| `social.disband_fellowship()` | break the player's fellowship up |
| `social.leave_fellowship()` | leave it |
| `social.recruit(id)` | ask a player into the fellowship |
| `social.dismiss(id)` | send a fellow out of it |
| `social.ask_allegiance(id)` | ask for the allegiance's profile about a member (nil: no longer); it comes as `notice.allegiance` and in `game.social` |
| `social.swear()` | swear an oath to the one selected |
| `social.break_with(id)` | break with a patron or a vassal |
| `social.add_friend(name)` | add a name to the friends |
| `social.remove_friend(name)` | take a name off them |
| `social.ask_friends()` | ask the chat service for the friends again |
| `social.set_state(state, location)` | set the player's own state on the chat service (`"available"`, `"away"` or `"busy"`) and where it is |
| `pets.attack(id)` | send the player's pets to attack an object |
| `pets.come_back()` | call them back |
| `pets.buff()` | have them strengthen the player |
| `pets.heal()` | have them heal the player |
| `pets.mode(id, mode)` | set a pet's mode, by its word (as `game.pets` gives it) or its number |
| `pets.rename(id, name)` | give a pet a name |
| `toolbar.show_set(n)` | show set `n` of ten slots (1 to 10) |
| `toolbar.put(slot, shortcut)` | put a shortcut in a slot (0 to 99), as `game.toolbar` gives one: `{ kind = "skill", skill = n }`, `{ kind = "item", item = id }`, `{ kind = "text", text = "…" }`, `{ kind = "emote", emote = n }` or `{ kind = "recipe", recipe = n }`; nil empties it |
| `craft.open(recipe)` | open a recipe in the making window |
| `craft.read(recipe, spinner)` | read a recipe at a spinner value, as the making window does (recipe 0: the window is closed); `game.craft().opened` says what it read |
| `craft.raise(skill)` | raise a craft skill |
| `craft.add(recipe, slot, item, spinner)` | put an item in a recipe's ingredient slot |
| `craft.make(recipe, spinner, ingredients)` | make a recipe from the items in its slots, a list of `{ slot, item }`; `craft.done` answers |

A control pressed is held, latched or single as the action map says. The player's body moves from
the controls in a client with a window, and in the headless client when it walks in the world
(`--walk`).

A camera a plugin chooses stands until the game's interface asks for another (a switch of interface
puts the interface's own back). In the headless client it turns the walk's camera, and the movement
controls move the body under it as in a client with a window.

`session.select_character(id)` enters the world with one of the listed characters; its promise
completes once the character is in the world, and fails with the reason when the server refuses the
entry. `session.logout()` completes back at the character list.

## Compiled plugins

A compiled plugin ([wasm.md](wasm.md)) for Auberean imports the operations every game shares, from the
WIT package `auberean:plugin`. Auberean's own operations above are Lua's for
now.

## Hosts

The desktop client runs plugins from the `plugins` folder beside its preferences: each package
there, a folder or a zip, is loaded as the client starts, and their stores are kept in
`plugin-data` beside it. A line the player types that a plugin claims (or `/plugin`) is the
plugin's, never said. The Horizon interface draws the plugins' windows and HUDs: a window opens
when its plugin shows it or when the player presses its button on the plugin bar (at the top
left), and closes from its close box (the plugin hears `ui.closed`); a press on a button or a
toggle is `ui.clicked`.

The headless client runs plugins: `--plugins <folder>` loads a package, or every package in a
folder; while it runs, it reads lines from its input: `plugin load <folder>`, `plugin unload <id>`,
`plugin reload <id>`, `plugin list`, `plugin console <id>`, `ui click <plugin> <window> <node>`,
`ui open <plugin> <window>`, and a line starting `/`, typed as the player would. The sample in
`auberean/plugins/effects` shows the shape of a plugin.

## What comes next

- More of the game to read: shops, the story's acts, the options.
- The plugin bar and windows drawn by the game's own interface (Horizon draws them).
- Auberean's operations for compiled plugins.
- Installing Auberean plugins from a plugin source (a list names each entry's game:
  [sources.md](sources.md)).
