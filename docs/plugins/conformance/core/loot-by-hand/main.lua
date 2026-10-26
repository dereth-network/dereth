-- A looter driven by a click: open the corpse, appraise what the profile cannot decide unseen,
-- and move each keeper to the pack, one request at a time, each awaited.
local loot = require("dereth.loot")
local rules = loot.load(files.package("rules.utl"))
local win = ui.window{ id = "main", title = "Looter", ui.button{ id = "loot", text = "Loot" } }
on("inventory.container_opened", function(e)
	print("opened", string.format("0x%08X", e.id))
end)
on("inventory.item_added", function(e)
	print("added", game.world:get(e.id).name)
end)
local function loot_container(cid)
	local kept = 0
	for _, item in ipairs(game.world:find{ container = cid }) do
		if rules:needs_appraisal(item) then
			await(act.appraise(item.id))
		end
		local hit = rules:classify(item)
		if hit and hit.keep then
			await(items.move(item.id))
			kept = kept + 1
			print("kept", item.name, hit.name)
		else
			print("left", item.name)
		end
	end
	print("done", kept)
end
on("ui.clicked", function(e)
	if e.id == "loot" then
		task.spawn(function()
			await(act.use(0x80000010))
			loot_container(0x80000010)
		end)
	end
end)
