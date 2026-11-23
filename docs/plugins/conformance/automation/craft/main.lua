-- What the character's items make is asked of a world that offers recipe answers, and of no other;
-- the server's craft question about the plugin's own combine comes to the plugin, with the chance,
-- and its answer goes to the server.
local HEADS, SHAFTS = 0x80000071, 0x80000072
on("session.login", function()
	task.spawn(function()
		print("before the offer", pawait(craft.recipes({ HEADS, SHAFTS })))
		print("not the character's", (pcall(craft.recipes, { 0x80000099 })))
		-- The offer comes at a second.
		task.sleep(1500)
		local ok, r = pawait(craft.recipes({ HEADS, SHAFTS }))
		print("answered", ok, #r, r[1].makes.name, r[1].chance, r[1].fails.item_lost)
		pawait(act.use_on(HEADS, SHAFTS))
	end)
end)
on("craft.confirm", function(e)
	task.spawn(function()
		print("asked", e.item == HEADS, e.target == SHAFTS, e.chance >= 0.9)
		print("confirmed", pawait(act.confirm(e.question, e.chance >= 0.9)))
		print("again", pawait(act.confirm(e.question, true)))
	end)
end)
