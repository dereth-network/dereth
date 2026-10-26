-- Spells are cast in magic stance only: a cast before it fails, and the library takes up the
-- caster and enters it. A heal is the heal domain's, only for a Life spell; the debuffer casts each
-- debuff once per target until its recast time has passed.
local fight = require("dereth.combat")
on("session.login", function()
	task.spawn(function()
		print("outside magic stance", pawait(magic.cast(15, nil, { purpose = "heal" })))
		print("magic stance", fight.ready_magic())
		print("heal", pawait(magic.cast(15, nil, { purpose = "heal" })))
		print("the same spell as a buff", pawait(magic.cast(15)))
		print("a Creature spell as a heal", (pcall(magic.cast, 1, nil, { purpose = "heal" })))
		local d = fight.debuffer({ spells = { 23 }, recast = 30 })
		print("debuff", pawait(d:next(0x80000060)))
		print("again at once", d:next(0x80000060))
		print("a target that is gone", d:next(0x80000099))
	end)
end)
