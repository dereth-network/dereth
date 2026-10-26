# Sample: Looter

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/looter/`](../../../../dereth/plugins/looter/).

| | |
|---|---|
| id | `network.dereth.looter`, version 0.2.0, API 0.7 |
| files | `plugin.toml`, `main.lua`, `rules.utl` (the loot profile it comes with) |
| permissions | `act.interactive` ("Opens the corpse and moves what your profile keeps when you click Loot."), `auto.loot` ("Loots each corpse you open, without a click, when Auto is ticked."), `plugins.call` ("Asks the Tank whether it is running, so the two never loot one corpse at once.") |
| capabilities | `ui.window` required |

## What it does

It loots a corpse or chest by a **loot profile**, the `.utl` rule files players already have. Click
**Loot** (or type `/loot`) and it opens the selected corpse or chest if none is open, appraises the
items the profile cannot decide from what every client sees, and moves each item the profile keeps
into the pack, one after the other. It counts what it kept per item name in the character's store.

With **Auto** ticked, it loots every corpse the player opens, with no
click: an automated action, which needs a world that allows automated looting (a world that
publishes no policy does). **While the Tank runs, Auto waits**: the Tank loots by its own profile,
so Looter asks the Tank's `running` service ([services.md](../../services.md)) before it loots a
corpse (and every two seconds while Auto is ticked), leaves the corpse to the Tank, and says so in
its window. A click on **Loot** still loots; once the Tank stops, Auto loots again.

Its **window**: **Loot** and **Auto**, what it is doing, the **Profile** (a choice of the `.utl`
files in its data folder and the `rules.utl` it comes with), and two tabs: **Kept** (what it has
kept, counted by item, most first, and the latest it kept with the rule that kept each) and
**Rules** (the chosen profile's rules, read only: each one's name, what it does, and its
requirements in plain words, `its value at least 2500`, `its class is gem`). The commands are
shortcuts to the same:

| command | does |
|---|---|
| `/loot` | loots the open container, or opens the selected corpse or chest and loots it |
| `/loot auto [on or off]` | ticks Auto, or not |
| `/loot profile <file>` | uses another profile, remembered per character |
| `/loot log` | how many of each item it has kept |

The `rules.utl` it comes with has three rules, named Valuables, Gems and Pyreals.

## The source

The profile is read with the host's loot library: at load the one it comes with, from the package;
then the one the character chose, from the data folder (`files.data`) or the package. The choice's
options are `rules.utl` and every `.utl` the data folder holds (`files.data_list()`):

```lua
local function profiles()
	local out = { BUNDLED }
	for _, f in ipairs(files.data_list()) do
		if string.lower(string.sub(f, -4)) == ".utl" and f ~= BUNDLED then
			table.insert(out, f)
		end
	end
	return out
end
```

The Rules tab reads the profile back with `profile:rules()`, each rule's `conditions` already in
plain words from the host:

```lua
	for _, r in ipairs(rules:rules()) do
		local does = r.action == "keep_up_to" and string.format("keep up to %d", r.count or 0) or r.action
		local when = #(r.conditions or {}) > 0 and table.concat(r.conditions, "; ") or "always"
		table.insert(described, { r.name, does, when })
	end
```

The window uses the brace form of a call, `ui.window{…}`, which is the same as `ui.window({…})`.

The loot itself runs as a task, so it can wait. Each action answers a promise at once; `pawait`
waits for it inside the task and answers `true, value` or `false, reason`. The host sends one
request at a time, so each move waits for the one before it to arrive.

```lua
local function loot_container(cid)
	if busy then
		return
	end
	busy = true
	local kept, seen = 0, 0
	for _, item in ipairs(game.world:find{ container = cid }) do
		seen = seen + 1
		if rules:needs_appraisal(item) then
			local ok = pawait(act.appraise(item.id))
			if not ok then
				print("could not appraise", item.name)
			end
		end
		local hit = rules:classify(item)
		if hit ~= nil and hit.keep then
			local ok, why = pawait(items.move(item.id))
			if ok then
				kept = kept + 1
				note(item.name, hit.name)
			else
				print("could not take", item.name, why)
			end
		end
	end
	status(string.format("Kept %d of %d.", kept, seen))
	busy = false
end
```

`game.world:find{ container = cid }` answers the items in the container. `rules:needs_appraisal`
says whether the profile can decide the item only after an appraisal; `rules:classify` answers the
first rule the item meets, or nil. `items.move(id)` with no container moves the item to the main
pack. An appraisal needs no permission; a move out of an open corpse is in the `loot` domain.

When nothing is open, the selected corpse or chest is opened first, and its promise tells whether
it opened:

```lua
	local ok, why = pawait(act.use(sel.id))
	if not ok then
		status("It would not open: " .. tostring(why))
		return
	end
	loot_container(sel.id)
```

A click on **Loot** is a player gesture: the plugin's actions for the next 30 seconds are
interactive. The handler starts the loot as a task, because a handler
itself cannot wait:

```lua
on("ui.clicked", function(e)
	if e.id == "loot" then
		task.spawn(loot_now)
	end
end)
```

**Auto** is the automated path: each corpse opened while nothing else is being looted, unless the
Tank says it is running. `services.call` needs `plugins.call`; a Tank not loaded fails the call as
`no_plugin`, which reads as not running:

```lua
local function ask_tank()
	local ok, running = pawait(services.call(TANK, "running"))
	tank_running = ok and running == true
	return tank_running
end

on("inventory.container_opened", function(e)
	local c = game.world:get(e.id)
	if not auto or c == nil or c.class ~= "Corpse" or busy then
		return
	end
	task.spawn(function()
		if ask_tank() then
			return
		end
		loot_container(e.id)
	end)
end)
```

## How it is tested

`dereth/testkit/tests/cpu/plugins.rs :: the_looter_keeps_what_its_profile_keeps_and_id_notices_reports_it`
loads it (with ID Notices and Trackers) in a scripted world with a corpse holding
pyreals, an opal worth 6,000 and a rusty dagger worth 5. One click on **Loot** moves the pyreals and
the opal to the pack, leaves the dagger, and the window says `Kept 2 of 3.`; ID Notices reports
`[ID] Opal: worth 6000`. The Core case `loot-by-hand` pins the order of the requests.

`dereth/testkit/tests/cpu/plugins/windows.rs :: the_looters_window_picks_a_profile_shows_its_rules_and_counts_what_it_kept`
lists a profile put in the data folder beside `rules.utl`, shows each profile's rules in words,
loots only the gem by the profile chosen in the window, and counts it on the Kept tab;
`the_looter_waits_while_the_tank_runs` opens a corpse while the Tank runs (left to it, the window
saying why), loots it by a click, and with the Tank stopped loots the next corpse opened.
