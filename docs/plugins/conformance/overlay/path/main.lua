-- Paths drawn in the world: a glow from the character along a way north, which the host starts at
-- the character, says how far along the way it starts (the pattern held on the ground), and
-- resamples a short step apart, and dashes along a way to the east drawn as they
-- are, on the ground, as far as their reach. A path of one point, a look not known, a picture the
-- plugin does not have, and a path over the plugin's sixteen are refused.
on("session.login", function()
	draw.path({ points = { { ns = 42.15, ew = 33.8 }, { ns = 42.25, ew = 33.8 }, { ns = 42.3, ew = 33.8 } } })
	draw.path({
		points = { { ns = 42.15, ew = 33.8 }, { ns = 42.15, ew = 33.9 } },
		look = "dashes",
		from_player = false,
		reach = 12,
		color = "#FFD700",
	})
	print("one point", (pcall(draw.path, { points = { { ns = 42.2, ew = 33.8 } } })))
	print("a look not known", (pcall(draw.path, {
		points = { { ns = 42.2, ew = 33.8 }, { ns = 42.3, ew = 33.8 } },
		look = "sparkles",
	})))
	print("a picture it does not have", (pcall(draw.path, {
		points = { { ns = 42.2, ew = 33.8 }, { ns = 42.3, ew = 33.8 } },
		image = "trail.png",
	})))
	local extra = {}
	for _ = 1, 15 do
		local ok, h = pcall(draw.path, {
			points = { { ns = 42.1, ew = 33.7 }, { ns = 42.1, ew = 33.71 } },
			from_player = false,
		})
		if ok then
			table.insert(extra, h)
		end
	end
	print("paths kept", 2 + #extra)
	for _, h in ipairs(extra) do
		h:remove()
	end
end)
