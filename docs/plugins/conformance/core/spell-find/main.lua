-- Spells found by name, case aside: every spell of that name, the character's known ones first,
-- then the rest, each in id order; a name no spell has finds none.
commands.add("find", "find spells by name", function()
	local function ids(list)
		local out = {}
		for _, s in ipairs(list) do
			table.insert(out, s.id .. (s.known and "+" or ""))
		end
		return table.concat(out, ",")
	end
	print("bolt", ids(magic.find("Flame Bolt I")))
	print("case", ids(magic.find("heal self i")))
	print("none", #magic.find("Fireworks"))
	local bolts = magic.find("Flame Bolt I")
	print("icons", string.format("0x%08X 0x%08X", bolts[1].icon, bolts[3].icon))
	local heal = magic.find("Heal Self I")[1]
	print("record", heal.name, heal.school, heal.icon == magic.spell(heal.id).icon)
end)
