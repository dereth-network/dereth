-- VTank's rule for a meta: every rule of the state whose condition holds runs, each once per stay in
-- the state; setting the state it is in enters it again, and its rules run again.
local meta = require("dereth.meta")
local n = 0
local m = meta.new({
	once = true,
	every = 100,
	start = "A",
	states = {
		A = {
			{ meta.always(), meta.lua(function()
				n = n + 1
				print("first", n)
			end) },
			{ meta.always(), meta.lua(function()
				print("second", n)
			end) },
			{ function()
				return n < 2
			end, meta.set_state("A") },
		},
	},
})
m:on_state(function(new, old)
	print("state", old, "->", new)
end)
on("session.login", function()
	m:start()
end)
