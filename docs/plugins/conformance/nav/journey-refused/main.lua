-- A journey whose first portal refuses the character: the refusal is learned and the journey
-- planned again through the other portal.
commands.add("go", "travel to the hideout", function()
	task.spawn(function()
		local r = await(nav.path_to({ cell = 0x019E0120, x = 20, y = -40, z = 0 }, {
			on_planned = function(e)
				for _, leg in ipairs(e.legs) do
					if leg.kind == "portal" then
						print("planned", leg.name, e.reason or "first")
					end
				end
			end,
			on_learned = function(e)
				print("learned", e.kind, e.name)
			end,
		}))
		print("arrived", r.distance <= 1)
		for _, p in ipairs(nav.known_portals()) do
			if p.retired ~= nil and p.source == "learned" then
				print("retired", p.name, p.source)
			end
		end
	end)
end)
