-- A world that asks for parity: a plugin finds only what the radar shows and what the player holds.
on("session.login", function()
	local names = {}
	for _, o in ipairs(game.world:find({})) do
		table.insert(names, o.name)
	end
	table.sort(names)
	print("found", table.concat(names, ","))
end)
