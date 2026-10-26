-- A message to this plugin's copies in the other clients on the machine, and what they hear.
commands.add("hello", "greet the other clients", function()
	peers.send("greet", { n = 1, from = game.player.name })
end)
on("peers.message", function(e)
	print("heard", e.from, e.channel, e.data.n, e.data.from)
end)
