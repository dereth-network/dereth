-- A path replaced by another: the one removed goes out where it was over half a second as the new
-- one comes in, each drawn at its share (the top byte of its colour) until it is fully in or gone.
local path = nil
on("session.login", function()
	path = draw.path({ points = { { ns = 42.2, ew = 33.8 }, { ns = 42.3, ew = 33.8 } }, from_player = false })
end)
commands.add("replace", "draw the way again", function()
	path:remove()
	path = draw.path({ points = { { ns = 42.2, ew = 33.8 }, { ns = 42.2, ew = 33.9 } }, from_player = false })
end)
