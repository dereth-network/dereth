-- An attack the client cannot make (not in combat), one the server refuses, and one the server
-- answers as it ends an attack sequence after its swing: each settles as what it was.
on("session.login", function()
	task.spawn(function()
		print("peace", pawait(combat.attack(0x80000050, { power = 0.5 })))
		print("mode", pawait(combat.set_mode("combat")))
		print("refused", pawait(combat.attack(0x80000050, { power = 0.5 })))
		local ok, how = pawait(combat.attack(0x80000050, { power = 0.5 }))
		print("swing", ok, ok and how.ended or how)
	end)
end)
