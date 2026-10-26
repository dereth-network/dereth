-- In a world that allows support and denies combat: the caster taken up and the magic stance
-- entered for support, a buff cast on another player, a debuff on a monster refused, peace again
-- for support; the stance without a purpose refused, and a purpose no spell has an error.
local fight = require("dereth.combat")
on("session.login", function()
	task.spawn(function()
		print("the magic stance for support", fight.ready_magic({ purpose = "support" }))
		print("a buff on another player", pawait(magic.cast(2, 0x50000070)))
		print("a debuff on a monster", pawait(magic.cast(23, 0x80000071)))
		print("peace for support", pawait(combat.set_mode("peace", { purpose = "support" })))
		print("the stance with no purpose", pawait(combat.set_mode("combat")))
		print("a purpose that is not a spell's", (pcall(combat.set_mode, "combat", { purpose = "loot" })))
	end)
end)
