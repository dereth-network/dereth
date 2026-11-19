-- Effects: counts the effects put on the player and taken off this session, says each in the
-- chat, and shows the counts in a small window and with /effects.

local on_now = {} -- effect number -> true while it is on the player
local added, removed = 0, 0

local win = nil
if host.has("ui.window") then
	win = ui.window({
		id = "main",
		title = "Effects",
		width = 22,
		ui.label({ id = "counts", text = "None yet." }),
		ui.button({ id = "reset", text = "Reset" }),
	})
end

local function counts()
	local n = 0
	for _ in pairs(on_now) do
		n = n + 1
	end
	return string.format("%d on now; %d put on, %d taken off.", n, added, removed)
end

local function show()
	if win ~= nil then
		win:get("counts").text = counts()
	end
end

on("effects.added", function(e)
	on_now[e.effect] = true
	added = added + 1
	chat.print("Effect " .. e.effect .. " is on.")
	show()
end)

on("effects.removed", function(e)
	on_now[e.effect] = nil
	removed = removed + 1
	chat.print("Effect " .. e.effect .. " is off.")
	show()
end)

on("ui.clicked", function(e)
	if e.id == "reset" then
		added, removed = 0, 0
		show()
	end
end)

commands.add("effects", "/effects shows the effects counted", function()
	chat.print(counts())
	if win ~= nil then
		show()
		win:show()
	end
end)
