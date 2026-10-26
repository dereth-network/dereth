-- The character store opens at login and keeps plain values.
print("before login", (pcall(function() return store.character.x end)))
on("session.login", function()
	store.character.visits = (store.character.visits or 0) + 1
	store.character.list = { "a", "b", { n = 3 } }
	local l = store.character.list
	print("visits", store.character.visits, #l, l[3].n)
	print("function refused", (pcall(function() store.character.f = print end)))
end)
