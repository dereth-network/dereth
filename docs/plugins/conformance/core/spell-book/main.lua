-- The spell book in id order, whole or a page at a time; a page past the end is empty. A spell
-- carries its category in the spell table (its family), and how long it lasts.
commands.add("book", "read the spell book", function()
	local function names(list)
		local out = {}
		for _, s in ipairs(list) do
			table.insert(out, s.name)
		end
		return table.concat(out, ",")
	end
	print("all", #magic.known())
	print("page", names(magic.known(3, 2)))
	print("last", names(magic.known(7, 5)))
	print("past", #magic.known(11, 5))
	print("category", magic.spell(1133).category, magic.known(6, 1)[1].category)
	print("duration", magic.spell(1133).duration)
end)
