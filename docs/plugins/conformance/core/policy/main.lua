-- The world's published policy as a plugin reads it.
on("session.world_policy", function(e)
	print("policy", e.published, e.rules.loot, e.rules.combat, e.rules.nav, e.rules.session)
end)
on("session.login", function()
	local p = policy.info()
	print("notes", p.notes, p.peers_local)
end)
