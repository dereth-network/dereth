-- Looter: loots a corpse or chest by a loot profile (the `.utl` files players already have), when
-- the player clicks Loot, or by itself when the player ticks Auto and has allowed it.
--
-- Its window: Loot and Auto, the profile (a choice of the profiles in its data folder and the one
-- it comes with), what it kept (counts, and the latest), and the chosen profile's rules in plain
-- words. Items the profile cannot decide from what every client sees are appraised first; the
-- others never are. What was kept is counted per character (the Mag-LootLogger part).
--
-- While the Tank runs it loots by its own profile, so Auto stands down: Looter asks the Tank's
-- `running` service, says why in its window, and loots again by itself once the Tank stops. A click
-- on Loot still loots.
--
--   /loot                  loot the open container (or the selected corpse)
--   /loot auto [on|off]    loot each corpse opened, or stop
--   /loot profile <file>   the loot profile
--   /loot log              what was kept, counted

local loot = require("dereth.loot")

local TANK = "network.dereth.tank"
-- The profile it comes with.
local BUNDLED = "rules.utl"

local profile_name = BUNDLED
local rules = loot.load(files.package(BUNDLED))
local auto = false
local busy = false
local log = {}
local status_text = "Open a corpse or chest, then Loot."
-- Whether the Tank says it is running, as last heard.
local tank_running = false

local win = ui.window{ id = "main", title = "Looter", width = 48,
	ui.row{
		ui.button{ id = "loot", text = "Loot" },
		ui.toggle{ id = "auto", text = "Auto", value = auto },
	},
	ui.label{ id = "status", text = status_text },
	ui.choice{ id = "profile", text = "Profile", options = { BUNDLED }, value = BUNDLED },
	ui.tabs{ id = "pages", selected = 1,
		ui.group{ title = "Kept",
			ui.list{ id = "kept", columns = { "Item", { title = "Kept", width = 6 } }, rows = {}, height = 6 },
			ui.list{ id = "log", columns = { "Latest", { title = "Rule", width = 14 } }, rows = {}, height = 4 },
		},
		ui.group{ title = "Rules",
			ui.list{ id = "rules", columns = { { title = "Rule", width = 14 }, { title = "Does", width = 12 }, "When" },
				rows = {}, height = 10 },
		},
	},
}

-- The profiles to choose from: the one it comes with, and every `.utl` in its data folder.
local function profiles()
	local out = { BUNDLED }
	for _, f in ipairs(files.data_list()) do
		if string.lower(string.sub(f, -4)) == ".utl" and f ~= BUNDLED then
			table.insert(out, f)
		end
	end
	return out
end

-- A profile's text: from the data folder, else the package.
local function profile_text(name)
	return files.data(name) or files.package(name)
end

local function kept_counts()
	local ok, counts = pcall(function()
		return store.character.kept
	end)
	return ok and counts or {}
end

local function refresh()
	local status = status_text
	if auto and tank_running then
		status = "Auto waits: the Tank is running and loots by its own profile."
	end
	win:get("status").text = status
	win:get("auto").value = auto
	local names = profiles()
	local listed = false
	for _, n in ipairs(names) do
		listed = listed or n == profile_name
	end
	if not listed then
		table.insert(names, profile_name)
	end
	win:get("profile").options = names
	win:get("profile").value = profile_name
	local rows = {}
	for name, n in pairs(kept_counts() or {}) do
		table.insert(rows, { name, n })
	end
	table.sort(rows, function(a, b)
		return a[2] > b[2] or (a[2] == b[2] and a[1] < b[1])
	end)
	win:get("kept").rows = rows
	win:get("log").rows = log
	local described = {}
	for _, r in ipairs(rules:rules()) do
		local does = r.action == "keep_up_to" and string.format("keep up to %d", r.count or 0) or r.action
		local when = #(r.conditions or {}) > 0 and table.concat(r.conditions, "; ") or "always"
		table.insert(described, { r.name, does, when })
	end
	win:get("rules").rows = described
end

local function status(text)
	status_text = text
	refresh()
end

-- Use profile `name`; answers whether it read.
local function use_profile(name)
	local text = profile_text(name)
	if text == nil then
		status("No profile " .. name .. " in the looter's folder.")
		return false
	end
	local ok, p = pcall(loot.load, text)
	if not ok then
		status("The profile " .. name .. " does not read: " .. tostring(p))
		return false
	end
	rules, profile_name = p, name
	pcall(function()
		store.character.profile = name
	end)
	status("Loot profile: " .. name .. ".")
	return true
end

-- The character's own choices: its profile and whether Auto is ticked.
on("session.login", function()
	local name = store.character.profile
	if name ~= nil and profile_text(name) ~= nil then
		use_profile(name)
	end
	auto = store.character.auto or false
	refresh()
end)

local function note(name, rule)
	table.insert(log, 1, { name, rule })
	while #log > 50 do
		table.remove(log)
	end
	local counts = store.character.kept or {}
	counts[name] = (counts[name] or 0) + 1
	store.character.kept = counts
	refresh()
end

-- Everything in `cid` the profile keeps, moved to the pack one at a time.
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

-- The container to loot: the open one, else the selected corpse or chest, opened first.
local function loot_now()
	local open = game.inventory.open_container
	if open ~= nil then
		loot_container(open)
		return
	end
	local sel = game.selection()
	if sel == nil or (sel.class ~= "Corpse" and sel.class ~= "Container") then
		status("Select or open a corpse or chest first.")
		return
	end
	local ok, why = pawait(act.use(sel.id))
	if not ok then
		status("It would not open: " .. tostring(why))
		return
	end
	loot_container(sel.id)
end

local function set_auto(on)
	auto = on
	pcall(function()
		store.character.auto = auto
	end)
	refresh()
end

-- Whether the Tank is running, by its service; not loaded or not answering, it is not.
local function ask_tank()
	local ok, running = pawait(services.call(TANK, "running"))
	local was = tank_running
	tank_running = ok and running == true
	if was ~= tank_running then
		print(tank_running and "the Tank is running: Auto waits" or "the Tank stopped: Auto loots again")
		refresh()
	end
	return tank_running
end

task.every(2000, function()
	if auto then
		task.spawn(ask_tank)
	end
end)

on("ui.clicked", function(e)
	if e.window == "main" and e.id == "loot" then
		task.spawn(loot_now)
	end
end)

on("ui.changed", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "auto" then
		set_auto(e.value)
		if auto then
			task.spawn(ask_tank)
		end
	elseif e.id == "profile" and e.value ~= profile_name then
		if not use_profile(e.value) then
			refresh()
		end
	end
end)

-- With Auto ticked, every corpse the player opens is looted: an action without a gesture, so it
-- needs a world that allows automated looting. While the Tank runs, it loots, not Looter.
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

commands.add("loot", "loot the open container; /loot auto [on|off]; /loot profile <file>; /loot log", function(args, words)
	if words[1] == "log" then
		local counts = kept_counts() or {}
		for name, n in pairs(counts) do
			chat.print(string.format("%s: %d", name, n))
		end
	elseif words[1] == "auto" then
		set_auto(words[2] == nil and not auto or words[2] == "on")
		chat.print("Looter: Auto is " .. (auto and "on" or "off") .. ".")
		if auto then
			task.spawn(ask_tank)
		end
	elseif words[1] == "profile" and words[2] ~= nil then
		if use_profile(table.concat(words, " ", 2)) then
			chat.print("Loot profile: " .. profile_name)
		end
	else
		task.spawn(loot_now)
	end
end)

refresh()
