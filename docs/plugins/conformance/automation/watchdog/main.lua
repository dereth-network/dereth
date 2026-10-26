-- A watchdog: the character does not move 2 m in 3 s, so the meta goes to its Stuck state. Set
-- again on every check, it keeps its first clock.
local meta = require("dereth.meta")
local m = meta.new({
	start = "Waiting",
	states = {
		Waiting = { { meta.always(), meta.watchdog_set("Stuck", 2, 3) } },
		Stuck = { { meta.always(), meta.set_state("Done") } },
		Done = {},
	},
})
m:on_state(function(new, old)
	print("state", old, "->", new, os.clock() >= 3.2 and os.clock() < 4)
end)
on("session.login", function()
	m:start()
end)
