-- Walking on its own: once at the start, which the player interrupts; again a second after the
-- player moved, which is refused; and again once the player has left the controls alone.
local function walk(label)
	task.spawn(function()
		print(label, pawait(nav.move_to({ ns = 42.2, ew = 34.2 }, 1)))
	end)
end
on("session.login", function()
	walk("first")
	task.after(3000, function()
		walk("soon after")
	end)
	task.after(9000, function()
		walk("later")
		task.after(500, function()
			print("moving", nav.owner())
		end)
	end)
end)
