-- Three stores: everywhere, this world, this character; each is held to its quota.
on("session.login", function()
	store.global.runs = (store.global.runs or 0) + 1
	store.world.seen = true
	store.character.name = game.player.name
	print("global", store.global.runs, "world", store.world.seen, "character", store.character.name)
	local ok, err = pcall(function()
		store.global.big = string.rep("x", 100)
	end)
	print("quota", ok, string.find(tostring(err), "limited to 64 bytes", 1, true) ~= nil)
	store.global.big = nil
	print("after", store.global.big, store.global.runs)
end)
