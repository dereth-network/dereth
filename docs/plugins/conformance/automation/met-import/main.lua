-- A VTank meta file, imported: its states and rules, an embedded route, an expression, an option
-- and a meta view whose button changes the state.
local meta = require("dereth.meta")
local m = meta.load_met(files.package("test.met"))
for _, u in ipairs(m.unsupported) do
	print("unsupported", u)
end
m:on_state(function(new, old)
	print("state", old, "->", new)
end)
m:on_option(function(name, value)
	print("option", name, value)
end)
on("session.login", function()
	m:start()
end)
