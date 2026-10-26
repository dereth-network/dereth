-- A VTank meta file read as data and written again: the same rules, its embedded route and meta
-- view kept as VTank writes them; a rule added as data, written and loaded, runs.
local meta = require("dereth.meta")
local file = meta.read_met(files.package("test.met"))
local text = meta.write_met(file)
local again = meta.read_met(text)
for i, r in ipairs(again.rules) do
	print("rule", i, r.state, r.condition.kind, r.action.kind)
end
print("unchanged", meta.write_met(again) == text)
print("line ends", string.find(text, "[^\r]\n") == nil)
-- The view's markup runs straight into the state after it.
print("view", string.find(text, "</view>s\r\nDefault\r\n", 1, true) ~= nil)
for _, r in ipairs(again.rules) do
	for _, a in ipairs(r.action.kind == "multiple" and r.action.data or {}) do
		if a.kind == "walk" then
			print("route", a.data.name, a.data.route.kind, #a.data.route.points)
		end
	end
end
-- A rule added at the head of Default: it runs before the file's own.
table.insert(again.rules, 1, {
	state = "Default",
	condition = { kind = "monster_count_within", data = { n = "Drudge", c = 0, r = 20.5 } },
	action = { kind = "set_state", data = "Added" },
})
local m = meta.load_met(meta.write_met(again))
m:on_state(function(new, old)
	print("state", old, "->", new)
end)
on("session.login", function()
	m:start()
end)
