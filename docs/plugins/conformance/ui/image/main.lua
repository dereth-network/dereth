-- A picture from the package, and one fetched as an image and shown by the name it answers with;
-- a picture over 1024 pixels a side is not kept, from the package or fetched.
local w = ui.window({
	id = "main",
	title = "Pictures",
	ui.image({ id = "compass", src = "compass.png", width = 8, height = 4 }),
	ui.image({ id = "map", text = "loading", width = 8, height = 4 }),
	ui.image({ id = "wide", src = "wide.png", text = "too wide", width = 8, height = 4 }),
})
task.spawn(function()
	local name = await(net.fetch({ url = "https://maps.example.org/town.png", as = "image" }))
	w:get("map").src = name
	print("fetched", name)
	print("not a picture", pawait(net.fetch({ url = "https://maps.example.org/notes.txt", as = "image" })))
	print("too wide", pawait(net.fetch({ url = "https://maps.example.org/wide.png", as = "image" })))
end)
