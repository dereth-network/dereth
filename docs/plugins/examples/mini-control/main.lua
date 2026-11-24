-- A narrow strip of switches at the screen's left edge: each a game icon, lit while on, dimmed
-- while off. A click on one turns it; /mini <n> turns the nth from a command (and /mini alone
-- shows the strip: a plugin's HUD opens on a player's gesture).

-- Each switch: what it is for, the spell whose icon it shows, its accent, and whether it starts on.
local switches = {
	{ id = "fight", tip = "Fighting", spell = "Flame Bolt I", color = "#FF7040", on = true },
	{ id = "buff", tip = "Buffing", spell = "Strength Self I", color = "#60D060", on = true },
	{ id = "heal", tip = "Healing", spell = "Heal Self I", color = "#FF5A8A", on = false },
	{ id = "move", tip = "Moving", spell = "Sprint Self I", color = "#40C0FF", on = true },
	{ id = "loot", tip = "Looting", spell = "Item Enchantment Mastery Self I", color = "#FFC040", on = false },
}

-- A spell's icon by its name: the first spell of that name, a known one when there is one.
local function icon(name)
	local found = magic.find(name)
	return found[1] and found[1].icon or 0
end

local function draw()
	local nodes = {}
	for _, s in ipairs(switches) do
		table.insert(nodes, ui.toggle{ id = s.id, icon = icon(s.spell), color = s.color,
			value = s.on, tooltip = s.tip })
	end
	ui.hud{ id = "strip", anchor = "left", ui.column{ gap = 2, table.unpack(nodes) } }
end

on("ui.changed", function(e)
	for _, s in ipairs(switches) do
		if s.id == e.node then
			s.on = e.value
		end
	end
	draw()
end)

commands.add("mini", "turn the nth switch", function(args)
	local s = switches[tonumber(args) or 0]
	if s ~= nil then
		s.on = not s.on
	end
	draw()
end)

draw()
