-- Hears the session's own events, in the order the client applied the messages behind them.
on("session.login", function(e) print("login", e.character) end)
on("session.enter_world", function() print("enter_world") end)
on("character.xp", function(e) print("xp", e.total, e.gained) end)
on("character.vital", function(e) print("vital", e.vital, e.current, e.max) end)
on("world.selection_changed", function(e) print("selection", e.id ~= nil) end)
