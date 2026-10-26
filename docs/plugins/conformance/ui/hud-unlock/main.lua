-- A HUD with a button: while the player has the HUDs unlocked, a press on it moves the HUD and
-- the plugin hears no click; locked again, the button answers.
ui.hud{ id = "hud", anchor = "top_right", ui.button{ id = "ping", text = "Ping" } }
on("ui.clicked", function(e)
	print("clicked", e.window, e.id)
end)
