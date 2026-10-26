# Sample: XP Meter

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/xp-meter/`](../../../../dereth/plugins/xp-meter/).

| | |
|---|---|
| id | `network.dereth.xpmeter`, version 0.1.0, API 0.1 |
| files | `plugin.toml`, `main.lua`, `format.lua` |
| permissions | none beyond `read` and `chat.print`, which every plugin has |
| capabilities | `ui.window` optional |

## What it does

It counts the experience the character earns from its first gain after login and works out an
hourly rate from it, so the minutes before the first kill do not dilute the rate. A small window
shows the session's total, the rate, the unassigned experience and how far the next level is; `/xp`
prints the same in the chat window with the total the plugin has watched over every session, which
it keeps in the character's store.

| command | does |
|---|---|
| `/xp` | `XP this session: 11 in 1 gains, 1,173 per hour. Watched in all: 11.` |
| `/xp reset` | starts the session count again |

The window's **Reset** button does the same as `/xp reset`. It is the smallest sample, and the
one to read first.

## The source

The manifest names the entry file and makes the window optional, so the plugin still loads (with
`/xp` alone) in a host that has no windows:

```toml
id = "network.dereth.xpmeter"
name = "XP Meter"
version = "0.1.0"
api = "0.1"
entry = "main.lua"
description = "Experience earned this session and per hour, in a small window and on /xp."
authors = ["Dereth"]
license = "MIT"

[capabilities]
optional = ["ui.window"]
```

`main.lua` starts by loading its helper module from its own package; `require` reaches nothing
else:

```lua
local fmt = require("format")

local session = { earned = 0, started = nil, gains = 0 }
local win = nil
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
		ui.button({ id = "reset", text = "Reset" }),
	})
	on("ui.clicked", function(e)
		if e.id == "reset" then
			reset()
		end
	end)
end
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
comes only after login.

```lua
on("character.xp", function(e)
	if e.gained <= 0 then
		return
	end
	if session.started == nil then
		session.started = os.clock()
	end
	session.earned = session.earned + e.gained
	session.gains = session.gains + 1
	store.character.lifetime = (store.character.lifetime or 0) + e.gained
	refresh()
end)
```

A login starts a new session, a timer keeps the rate current once a second, and the command reads
its argument from `args`, the rest of the typed line:

```lua
on("session.login", function()
	reset()
end)

task.every(1000, refresh)

commands.add("xp", "/xp shows experience this session and the rate; /xp reset starts again", function(args)
	if args == "reset" then
		reset()
		chat.print("XP Meter reset.", "system")
		return
	end
```

## How it is tested

`dereth/testkit/tests/cpu/plugins.rs :: a_compiled_plugin_counts_what_the_same_lua_plugin_counts`
loads this plugin and the test suite's compiled copy of it over the same recorded session and
requires the same answer to `/xp` and `/xpc`, and the same total in both windows.
