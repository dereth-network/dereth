-- An appraisal is a promise: it settles with what the appraisal told once the client has it, the
-- appraised event reaches every plugin that listens, and an appraisal nobody answers fails as a
-- timeout.
on("items.appraised", function(e)
	print("appraised", string.format("0x%08X", e.id))
end)
commands.add("id", "appraise the ring", function()
	task.spawn(function()
		local a = await(act.appraise(0x80000011))
		print("workmanship", a.ints[105], "spells", #a.spells)
		local ring = game.world:get(0x80000011)
		print("class", ring.class, "int", ring:int(105), "value", ring:int(19))
		local ok, why = pawait(act.appraise(0x80000099))
		print("unknown", ok, why)
	end)
end)
