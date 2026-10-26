-- A plugin whose manifest names no permission is asked nothing and may do what each permission
-- covers, with or without a gesture in a world that publishes no policy; only a request over the
-- network needs its manifest to name net.http.
for _, p in ipairs({ "read.chat.private", "messages", "chat.send", "chat.filter", "auto.craft",
	"auto.combat", "session.control", "peers.local", "plugins.call", "files.import", "retail_ui",
	"net.http" }) do
	print(p, permissions.has(p))
end
on("session.login", function()
	task.spawn(function()
		print("automated move", pawait(items.move(0x80000021, 0x50000001)))
	end)
end)
