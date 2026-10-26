-- Portal space and death, heard as events and as a meta's conditions.
local meta = require("dereth.meta")
on("session.portal_enter", function()
	print("event", "portal_enter")
end)
on("session.portal_exit", function()
	print("event", "portal_exit")
end)
on("character.death", function(e)
	print("event", "death", e.deaths)
end)
local m = meta.new({
	start = "Hunting",
	states = {
		Hunting = {
			{ meta.portal_entered(), meta.set_state("Portal") },
			{ meta.died(), meta.set_state("Dead") },
		},
		Portal = {
			{ meta.in_portal(), meta.lua(function(ctx)
				if not ctx.meta.said then
					ctx.meta.said = true
					print("in portal space")
				end
			end) },
			{ meta.portal_exited(), meta.set_state("Arrived") },
		},
		Arrived = { { meta.died(), meta.set_state("Dead") } },
		Dead = {},
	},
})
m:on_state(function(new, old)
	print("state", old, "->", new)
end)
on("session.login", function()
	m:start()
end)
