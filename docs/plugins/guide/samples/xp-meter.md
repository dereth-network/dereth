# Sample: XP Meter

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/xp-meter/`](../../../../dereth/plugins/xp-meter/).

| | |
|---|---|
| id | `network.dereth.xpmeter`, version 0.2.0, API 0.7 |
| files | `plugin.toml`, `main.lua`, `format.lua`, `monsters.lua` |
| permissions | `messages`, which every plugin has, named to say why it reads the message stream |
| capabilities | `ui.window` and `messages`, both optional |

## What it does

It counts the experience the character earns from its first gain after login and works out an
hourly rate from it, so the minutes before the first kill do not dilute the rate. A small window
shows the session's total, the rate, the unassigned experience and how far the next level is, and
the session's kills, deaths and luminance, each with its rate per hour since the first of them;
`/xp` prints the same in the chat window with the total the plugin has watched over every session,
which it keeps in the character's store.

Its **Monsters** button opens a second window: each monster the character has fought, by name, with
its kills, the experience a kill gave, how often it evaded the character and the character it, and
the damage each way. Below the list it shows the rest for the monster picked (at first the one
killed most): how often it resisted the character's spells and the character its, plain and critical
hits apart, spell damage each way, seconds and casts a kill, and the time from its death to its
corpse. **Forget**, beside its name, forgets that monster alone. Nothing about any monster ships
with the plugin; it learns all of it from play and keeps it with the character.

| command | does |
|---|---|
| `/xp` | opens the window and prints `XP this session: 11 in 1 gains, 1,173 per hour. Watched in all: 11.` then `Kills: 1 (107 / hour), deaths: 0 (0 / hour), luminance: 0 (0 / hour).` |
| `/xp reset` | starts the session count again; the monsters stay |
| `/xp monsters` | opens the Monsters window and prints a line for each of the ten monsters killed most: `Sparring Golem: 2 kills, 14 XP a kill, evades you 22%, you evade it 58%, resists you 0%, 52.0 s a kill.` |
| `/xp forget` | forgets the monsters; `/xp forget Drudge Skulker` forgets that one alone, its name as its row shows it (case aside) |

The window's **Reset** button does the same as `/xp reset`. Its first part (`main.lua` without the
monsters) is the smallest sample, and the one to read first.

## The source

The manifest names the entry file and makes the window and the message stream optional, so the
plugin still loads (with `/xp` alone) in a host that has no windows. It names the `messages`
permission with the reason the player is shown:

```toml
id = "network.dereth.xpmeter"
name = "XP Meter"
version = "0.2.0"
api = "0.7"
entry = "main.lua"
description = "Experience, kills, deaths and luminance this session and per hour, and what each monster you fight does, in two small windows and on /xp."
authors = ["Dereth"]
license = "MIT"

[permissions]
"messages" = "Reads the server's combat notices, to count what each monster you fight does."

[capabilities]
optional = ["ui.window", "messages"]
```

`main.lua` starts by loading its helper modules from its own package; `require` reaches nothing
else:

```lua
local fmt = require("format")
local monsters = require("monsters")

local session = { earned = 0, started = nil, gains = 0 }
local tally = { kills = 0, deaths = 0, luminance = 0, since = nil }
```

`format.lua` is a plain module that returns a table; `fmt.number` puts thousands separators into a
rounded number (`1234567` becomes `"1,234,567"`).

The rate is the session's experience over the hours since the first gain. `os.clock()` is the
plugin's own clock: seconds since it loaded, on the frame clock, so a replay gives the same rate.

```lua
local function rate()
	if session.started == nil then
		return 0
	end
	local hours = (os.clock() - session.started) / 3600
	if hours <= 0 then
		return 0
	end
	return session.earned / hours
end
```

The window is declared only when the host has windows. `ui.window` takes the window's properties
and, in its list part, its nodes; the labels the plugin will change later have ids.

```lua
if host.has("ui.window") then
	win = ui.window({
		id = "main",
		title = "XP Meter",
		width = 24,
		ui.row({ ui.label({ text = "This session:" }), ui.label({ id = "earned", text = "0" }) }),
		ui.row({ ui.label({ text = "Rate:" }), ui.label({ id = "rate", text = "0 / hour" }) }),
		ui.row({ ui.label({ text = "Unassigned:" }), ui.label({ id = "unassigned", text = "-" }) }),
		ui.label({ id = "level", text = "" }),
		ui.row({ ui.label({ text = "Kills:" }), ui.label({ id = "kills", text = "0 (0 / hour)" }) }),
		ui.row({ ui.label({ text = "Deaths:" }), ui.label({ id = "deaths", text = "0 (0 / hour)" }) }),
		ui.row({ ui.label({ text = "Luminance:" }), ui.label({ id = "luminance", text = "0 (0 / hour)" }) }),
		ui.row({ ui.button({ id = "reset", text = "Reset" }), ui.button({ id = "monsters", text = "Monsters" }) }),
	})
```

A press on **Monsters** is the player asking, so `mon:show()` opens the second window then; a
window a plugin shows of its own accord stays closed.

```lua
	on("ui.clicked", function(e)
		if e.id == "reset" then
			reset()
		elseif e.id == "monsters" then
			refresh_monsters()
			mon:show()
		end
	end)
```

`refresh` writes the numbers into the window by setting node properties, and reads
`game.player.xp` for what the event does not carry (`to_level` is nil where the host has no
experience table):

```lua
		win:get("earned").text = fmt.number(session.earned)
		win:get("rate").text = fmt.number(rate()) .. " / hour"
		local xp = game.player.xp
		if xp ~= nil then
			win:get("unassigned").text = fmt.number(xp.unassigned)
```

The heart of it is one handler. `character.xp` carries `gained`, the change in total experience;
it is 0 when only the unassigned experience moved (spending it on a skill), which the plugin
ignores. The lifetime total goes to `store.character`, which is open because an experience change
comes only after login. The gain goes to the monster statistics too, which give it to a kill just
before it.

```lua
on("character.xp", function(e)
	if e.gained <= 0 then
		return
	end
	if session.started == nil then
		session.started = os.clock()
	end
	counted()
	session.earned = session.earned + e.gained
	session.gains = session.gains + 1
	store.character.lifetime = (store.character.lifetime or 0) + e.gained
	if book ~= nil then
		book.gained(e.gained)
	end
	refresh()
end)
```

`character.luminance` and `character.death` count the session's luminance and deaths the same
way. A login starts a new session and opens the monster statistics the character keeps, a timer
keeps the rates current once a second, and the command reads its argument from `args`, the rest
of the typed line:

```lua
on("session.login", function()
	reset()
	book = monsters.new(store.character.monsters, os.clock)
	picked = nil
	refresh()
end)
```

### Monster statistics

`monsters.lua` keeps a table of counts for each monster's name: kills, experience, hits and their
damage each way, evasions each way, spells each way, casts, and the fights' and corpses' seconds.
What happened comes from three places:

- **The message stream** ([messages](../../messages.md)): the server tells each swing's outcome in
  a combat notice naming the other side, a hit with its damage (`Combat_HandleAttackerNotificationEvent`,
  `Combat_HandleDefenderNotificationEvent`) or an evasion (`Combat_HandleEvasionAttackerNotificationEvent`,
  `Combat_HandleEvasionDefenderNotificationEvent`), and tells the killer of each monster in a line
  (`Combat_HandleVictimNotificationEventOther`). The stream also has the character's own casts
  (`Magic_CastTargetedSpell`, sent), each naming its target.
- **Chat lines:** a spell's damage, a resist and a spell taking hold are told only in the server's
  magic lines (`You scorch Drudge Skulker for 22 points with Flame Bolt I.`,
  `Drudge Skulker resists your spell`, `You cast Imperil Other I on Drudge Skulker`).
- **Events:** `character.xp` for a kill's experience, and `world.object_created` for its corpse
  (`Corpse of Drudge Skulker`).

```lua
	messages.on("Combat_HandleAttackerNotificationEvent", heard(function(f)
		book.hit(f.defender_name, f.damage, f.critical ~= 0)
	end))
```

The killer's line names the monster inside a sentence (`Drudge Skulker falls before your blow!`,
`Carpenter Wasp is reduced to cinders!`), so the plugin finds in it the longest name it knows: the
monsters it has counted and those `game.world:find{ monster = true }` gives around the character.
A monster killed by one spell and no swing is still named so.

```lua
function M.victim(text, names)
	local best = nil
	for _, n in ipairs(names) do
		if n ~= "" and string.find(text, n, 1, true) and (best == nil or #n > #best) then
			best = n
		end
	end
	return best
end
```

The experience a kill gives arrives with its line or soon after: a gain within three seconds of a
kill is that kill's (the oldest kill still waiting), and a kill that waits longer gave none. A
fight with a monster of a name runs from the first thing that happened with one to its kill, and
a corpse of its name within thirty seconds of the kill gives its time to corpse.

A store hands a plugin a copy of what it holds, so the plugin keeps the counts in its own table
and writes the whole table back once a second while it has changed, and at logout and unload:

```lua
local function save()
	if book ~= nil and book.dirty then
		store.character.monsters = book.book
		book.dirty = false
	end
end
```

## How it is tested

`dereth/testkit/tests/cpu/plugins.rs :: a_compiled_plugin_counts_what_the_same_lua_plugin_counts`
loads this plugin and the test suite's compiled copy of it over the same recorded session and
requires the same answer to `/xp` and `/xpc`, and the same total in both windows.

`dereth/testkit/tests/cpu/plugins/xp_meter.rs` runs the monster statistics over a recorded session
(five kinds of monster, twelve kills, swords and spells) and requires each kind's line; in a
scripted world it checks the experience and the corpse given to a kill, the casts counted, the
session's kills, deaths and luminance per hour, and the monsters kept for the character into its
next session.
