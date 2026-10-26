-- Follow: multi-box following between the player's clients on one machine.
--
-- Its window: Lead, a choice of the player's other clients (the characters the other copies of
-- Follow on this machine say they are) with Follow, Stop, what it is doing now, and a line sent to
-- every follower. The commands are shortcuts:
--
-- /follow lead            this client leads: it tells the others where its character is
-- /follow <leader>        this client follows the leading character
-- /follow stop            stop leading or following
-- /follow all <line>      (at the leader) every follower types <line>
--
-- A follower that can see the leader follows its character; one that cannot walks to where the
-- leader last said it was, which is movement with no gesture behind it (where the world allows
-- automated navigation).

-- How long a client is listed after it last said where it is, in seconds.
local HEARD_FOR = 10

local leading = false
local leader = nil -- the leader's character name, when following
local following = false
local latest = {} -- each leading character's last message, by lowercase name
local heard = {} -- each other client's character, by lowercase name: { name, at }
local state = "Neither leading nor following."
local now = 0

local win = ui.window({
	id = "main",
	title = "Follow",
	width = 40,
	ui.label({ id = "state", text = state }),
	ui.row({
		ui.button({ id = "lead", text = "Lead" }),
		ui.button({ id = "stop", text = "Stop" }),
	}),
	ui.row({
		ui.choice({ id = "leader", text = "Leader", options = { "" }, value = "", grow = 1 }),
		ui.button({ id = "follow", text = "Follow" }),
	}),
	ui.row({
		ui.input({ id = "line", text = "To all followers", value = "", placeholder = "/fellowship recruit", grow = 1 }),
		ui.button({ id = "send", text = "Send" }),
	}),
})

-- The other clients heard from lately, by name.
local function others()
	local out = {}
	for _, h in pairs(heard) do
		if now - h.at <= HEARD_FOR then
			table.insert(out, h.name)
		end
	end
	table.sort(out)
	return out
end

local function refresh()
	local options = { "" }
	for _, n in ipairs(others()) do
		table.insert(options, n)
	end
	-- The client being followed, else the one the player picked and has not followed yet.
	local value = leader or win:get("leader").value or ""
	local listed = value == ""
	for _, o in ipairs(options) do
		listed = listed or o == value
	end
	if not listed then
		table.insert(options, value)
	end
	win:get("leader").options = options
	win:get("leader").value = value
	win:get("state").text = state
	win:get("send").enabled = leading
	win:get("line").enabled = leading
end

local function set_state(text)
	state = text
	refresh()
end

-- Each client says where its character is twice a second, so every other copy lists it and a
-- follower can find its leader.
task.every(500, function()
	now = now + 0.5
	if game.player.id ~= nil then
		peers.send("where", {
			id = game.player.id,
			name = game.player.name,
			position = game.player.position,
			leading = leading,
		})
	end
	refresh()
end)

local function chase(msg)
	if following then
		return
	end
	local seen = msg.id and game.world:get(msg.id)
	if seen ~= nil then
		following = true
		set_state("Following " .. tostring(leader) .. ".")
		task.spawn(function()
			local _, why = pawait(nav.follow(msg.id, 3))
			following = false
			print("follow ended", why)
			if leader ~= nil then
				set_state("Following " .. leader .. ": " .. tostring(why) .. "; waiting for it.")
			end
		end)
	elseif msg.position ~= nil and nav.owner() == nil then
		set_state("Following " .. tostring(leader) .. ": walking to where it was last.")
		task.spawn(function()
			print("walked to leader", pawait(nav.move_to(msg.position, 3)))
		end)
	end
end

on("peers.message", function(e)
	if e.channel == "where" and type(e.data) == "table" and e.from ~= "" then
		latest[string.lower(e.from)] = e.data
		heard[string.lower(e.from)] = { name = e.from, at = now }
	end
	-- Only the leader's own messages are followed: a client that is not leading any more is not.
	if leader == nil or string.lower(e.from) ~= string.lower(leader) then
		return
	end
	if e.channel == "where" and type(e.data) == "table" and e.data.leading ~= false then
		chase(e.data)
	elseif e.channel == "all" and type(e.data) == "string" then
		chat.command(e.data)
	end
end)

local function lead()
	leading, leader = true, nil
	set_state("Leading: your other clients may follow " .. tostring(game.player.name) .. ".")
	chat.print("[Follow] Leading.")
end

local function stop()
	leading, leader = false, nil
	nav.stop()
	set_state("Neither leading nor following.")
	chat.print("[Follow] Stopped.")
end

local function follow(name)
	if name == nil or name == "" then
		set_state("Choose a client to follow.")
		return
	end
	leader = name
	leading = false
	win:get("leader").value = name
	set_state("Following " .. leader .. ".")
	chat.print("[Follow] Following " .. leader .. ".")
	if latest[string.lower(leader)] ~= nil then
		chase(latest[string.lower(leader)])
	end
end

local function send_all(line)
	if line == nil or line == "" then
		return
	end
	if not leading then
		set_state("Only the leader sends to its followers.")
		return
	end
	peers.send("all", line)
	set_state("Leading: sent " .. line .. " to the followers.")
end

on("ui.clicked", function(e)
	if e.window ~= "main" then
		return
	end
	if e.id == "lead" then
		lead()
	elseif e.id == "stop" then
		stop()
	elseif e.id == "follow" then
		follow(win:get("leader").value)
	elseif e.id == "send" then
		send_all(win:get("line").value)
		win:get("line").value = ""
	end
end)

commands.add("follow", "/follow lead | <leader> | stop | all <line>", function(args, words)
	if words[1] == "lead" then
		lead()
	elseif words[1] == "stop" then
		stop()
	elseif words[1] == "all" then
		send_all(string.match(args, "^%s*all%s+(.+)$"))
	elseif words[1] then
		follow(args)
	end
end)

refresh()
