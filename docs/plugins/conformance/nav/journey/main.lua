-- A typed command travels into a dungeon: the journey walks to the portal, uses it and walks on;
-- the plugin hears the plan, each leg and what the portal's use taught, then reads the store.
commands.add("go", "travel to the hideout", function()
	task.spawn(function()
		local r = await(nav.path_to({ cell = 0x019E0120, x = 20, y = -40, z = 0 }, {
			on_planned = function(e)
				local kinds = {}
				for i, leg in ipairs(e.legs) do
					kinds[i] = leg.kind
				end
				print("planned", table.concat(kinds, " "))
			end,
			on_leg = function(e)
				print("leg", e.index, e.kind, e.name or "")
			end,
			on_learned = function(e)
				print("learned", e.kind, e.name, string.format("0x%08X", e.to.cell))
			end,
		}))
		print("arrived", r.distance <= 1, r.portals)
		for _, p in ipairs(nav.known_portals()) do
			print("known", p.name, p.source, p.uses)
		end
	end)
end)
