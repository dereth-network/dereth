-- A line a plugin sends is never a command: not its own, not another plugin's and not the host's,
-- however it is spelled (a full-width slash or letters, an @, a comma after the verb); a line
-- sent before its verb is claimed is not one either. Only the player's typing is a gesture.
commands.add("conf", "a command the player types", function(args)
	print("conf ran", args, permissions.activation() > 29)
end)
on("session.login", function()
	for _, line in ipairs({
		"/conf,",
		"／plugin unload conformance.chatsend",
		"/ｐｌｕｇｉｎ unload conformance.chatsend",
		"＠plugin unload conformance.chatsend",
		"@PLUGIN, unload conformance.chatsend",
	}) do
		local ok, err = pcall(chat.say, line)
		print("send", ok, string.find(tostring(err), "plugin's command", 1, true) ~= nil)
	end
	print("send later", (pcall(chat.say, "/later now")))
	commands.add("later", "claimed after its line was sent", function(args)
		print("later ran", args)
	end)
	task.after(1000, function()
		print("activation", permissions.activation())
	end)
end)
