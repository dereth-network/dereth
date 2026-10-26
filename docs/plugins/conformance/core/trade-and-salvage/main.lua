-- Trade and salvage at the server's word: the vendor's wares opened, an item sold, five of a
-- stackable bought (five, not one), an item salvaged into a bag (settled by the item leaving the
-- pack, which is the server's answer), and the loot profile's salvage combining read.
local loot = require("dereth.loot")
local rules = loot.load(files.package("rules.utl"))
local function ranges(rs)
	local out = {}
	for _, r in ipairs(rs) do
		table.insert(out, r[1] .. "-" .. r[2])
	end
	return table.concat(out, ",")
end
local c = rules:salvage_combine()
print("combine", ranges(c.default), c.materials[1].material, ranges(c.materials[1].ranges))
local function held(name)
	local n = 0
	for _, it in ipairs(game.inventory.items()) do
		if it.name == name then
			n = n + (it.stack or 1)
		end
	end
	return n
end
commands.add("go", "trade and salvage", function()
	task.spawn(function()
		print("open", pawait(act.use(0x80000030)))
		local wares = {}
		for _, r in ipairs(vendor.info().stock) do
			table.insert(wares, r.name .. " " .. r.price)
		end
		print("wares", table.concat(wares, ", "))
		print("sell", pawait(vendor.sell({ 0x80000033 })))
		print("buy", pawait(vendor.buy({ { id = 0x80000031, count = 5 } })))
		print("tapers", held("Prismatic Taper"))
		print("salvage", pawait(items.salvage({ 0x80000034 })))
		for _, it in ipairs(game.inventory.items()) do
			if it.class == "Salvage" then
				print("bag", it.material, it.uses)
			end
		end
	end)
end)
