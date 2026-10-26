-- A meta: idle until a monster is near, then walk to it; back to idle once there.
local meta = require("dereth.meta")
local m = meta.new({
	start = "Idle",
	states = {
		Idle = {
			{ meta.monsters_within(1, 50), meta.set_state("Approach") },
		},
		Approach = {
			{ meta.seconds_in_state_ge(0), meta.lua(function(ctx)
				if not ctx.meta.moving then
					ctx.meta.moving = true
					task.spawn(function()
						local ok, r = pawait(nav.move_to(0x80000070, 3))
						print("reached", ok, r.distance <= 3)
						ctx.meta:set_state("Done")
					end)
				end
			end) },
		},
		Done = {},
	},
})
m:on_state(function(new, old)
	print("state", old, "->", new)
end)
on("session.login", function()
	m:start()
end)
