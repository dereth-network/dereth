-- Auberean's layer of the plugin API's Lua-level bindings: reading the game (the player, the
-- objects around, the selection, the inventory, the effects, the skills, the clocks, the game's
-- questions, the place line, the fellowship, allegiance and friends, the journal, the map's
-- markers, the pets, the shortcut bar, crafting) and the game's own actions (selecting, using,
-- attacking, the controls, the camera, the chat windows' settings, answering questions and offers,
-- trades, the fellowship, allegiance and friends, the pets, the shortcut bar, crafting) over the
-- bindings every game shares.
--
-- This chunk runs once per VM, after the shared bindings and before the sandbox is sealed. It
-- receives the API table, which it adds to, and the shared bindings' kit. An object of the game is
-- named by its id as text ("0x" and sixteen hex digits), as the events give it.

local api, kit = ...
local hostcall = kit.hostcall

-- Reading the game: each answers from the game's view as it is now, as the game's interfaces read
-- it.
api.game = {
	player = function()
		return hostcall("game.player")
	end,
	objects = function()
		return hostcall("game.objects")
	end,
	object = function(id)
		return hostcall("game.object", id)
	end,
	selection = function()
		return hostcall("game.selection")
	end,
	inventory = function()
		return hostcall("game.inventory")
	end,
	effects = function()
		return hostcall("game.effects")
	end,
	skills = function()
		return hostcall("game.skills")
	end,
	clock = function()
		return hostcall("game.clock")
	end,
	questions = function()
		return hostcall("game.questions")
	end,
	place = function()
		return hostcall("game.place")
	end,
	social = function()
		return hostcall("game.social")
	end,
	journal = function()
		return hostcall("game.journal")
	end,
	markers = function()
		return hostcall("game.markers")
	end,
	pets = function()
		return hostcall("game.pets")
	end,
	toolbar = function()
		return hostcall("game.toolbar")
	end,
	craft = function()
		return hostcall("game.craft")
	end,
}

-- Actions: each is carried out as the frame ends; what follows arrives as events.
api.act = {
	-- `act.control(id)` presses a control of the game's action map as its key would;
	-- `act.control(id, false)` lets it go; a number is the press's extent.
	control = function(id, extent)
		hostcall("game.control", id, extent)
	end,
	release_controls = function(typing)
		hostcall("game.release_controls", typing == true)
	end,
	select = function(id)
		hostcall("game.select", id)
	end,
	-- `act.select_next("foe")` selects the next foe around, as the selection key does;
	-- `act.select_next("foe", true)` the one before. The kinds: creature, item, player, foe,
	-- fellow, pet.
	select_next = function(kind, previous)
		hostcall("game.select_next", kind, previous == true)
	end,
	select_nearest = function(kind)
		hostcall("game.select_nearest", kind)
	end,
	select_self = function()
		hostcall("game.select_self")
	end,
	select_last = function()
		hostcall("game.select_last")
	end,
	select_last_attacker = function()
		hostcall("game.select_last_attacker")
	end,
	-- `act.camera({ movement = "character" })` chooses the orbit camera; `act.camera(nil)` the
	-- game's own.
	camera = function(settings)
		hostcall("game.camera", settings)
	end,
	look_at = function(id)
		hostcall("game.look_at", id)
	end,
	use_selected = function()
		hostcall("game.use_selected")
	end,
	use_slot = function(slot)
		hostcall("game.use_slot", slot)
	end,
	attack = function(start)
		hostcall("game.attack", start ~= false)
	end,
	cease_attack = function(id)
		hostcall("game.cease_attack", id)
	end,
	show_names = function(on)
		hostcall("game.show_names", on ~= false)
	end,
	-- `act.answer(id, true)` answers one of the game's open questions yes; `false`, no.
	answer = function(id, yes)
		hostcall("game.answer", id, yes ~= false)
	end,
}

-- The chat windows' settings: `chat.window(1, { opacity = 0.5, filter = 3, devote = 2 })`,
-- `chat.kind(4, { size = 2, color = 0xFFFFFF, popup = true })`, then `chat.keep()` to have the
-- server keep them.
api.chat.window = function(window, settings)
	hostcall("game.chat_window", window, settings or {})
end
api.chat.kind = function(kind, settings)
	hostcall("game.chat_kind", kind, settings or {})
end
api.chat.keep = function()
	hostcall("game.chat_keep")
end

-- The fellowship, the allegiance and the friends, and answering what other players offer.
api.social = {
	answer_fellowship = function(accept)
		hostcall("game.fellowship_answer", accept ~= false)
	end,
	answer_oath = function(accept)
		hostcall("game.oath_answer", accept ~= false)
	end,
	create_fellowship = function(name, share_experience, looting)
		hostcall("game.create_fellowship", name, share_experience ~= false, looting)
	end,
	disband_fellowship = function()
		hostcall("game.disband_fellowship")
	end,
	leave_fellowship = function()
		hostcall("game.leave_fellowship")
	end,
	recruit = function(id)
		hostcall("game.recruit", id)
	end,
	dismiss = function(id)
		hostcall("game.dismiss", id)
	end,
	ask_allegiance = function(id)
		hostcall("game.ask_allegiance", id)
	end,
	swear = function()
		hostcall("game.swear")
	end,
	break_with = function(id)
		hostcall("game.break_oath", id)
	end,
	add_friend = function(name)
		hostcall("game.add_friend", name)
	end,
	remove_friend = function(name)
		hostcall("game.remove_friend", name)
	end,
	ask_friends = function()
		hostcall("game.ask_friends")
	end,
	set_state = function(state, location)
		hostcall("game.chat_state", state, location)
	end,
}

-- The player's pets: `pets.attack(id)`, `pets.come_back()`, `pets.mode(pet, "idle")`.
api.pets = {
	attack = function(id)
		hostcall("game.pet_attack", id)
	end,
	come_back = function()
		hostcall("game.pet_return")
	end,
	buff = function()
		hostcall("game.pet_buff")
	end,
	heal = function()
		hostcall("game.pet_heal")
	end,
	mode = function(pet, mode)
		hostcall("game.pet_mode", pet, mode)
	end,
	rename = function(pet, name)
		hostcall("game.pet_name", pet, name)
	end,
}

-- The shortcut bar: `toolbar.put(3, { kind = "skill", skill = 0x10 })`, `toolbar.put(3, nil)`
-- empties the slot, `toolbar.show_set(2)`.
api.toolbar = {
	show_set = function(set)
		hostcall("game.toolbar_set", set)
	end,
	put = function(slot, shortcut)
		hostcall("game.toolbar_slot", slot, shortcut)
	end,
}

-- Crafting: `craft.open(recipe)`, `craft.add(recipe, slot, item, spinner)`,
-- `craft.make(recipe, spinner, { { slot = 1, item = id } })`.
api.craft = {
	open = function(recipe)
		hostcall("game.craft_open", recipe)
	end,
	read = function(recipe, spinner)
		hostcall("game.craft_read", recipe, spinner)
	end,
	raise = function(skill)
		hostcall("game.craft_raise", skill)
	end,
	add = function(recipe, slot, item, spinner)
		hostcall("game.craft_add", recipe, slot, item, spinner)
	end,
	make = function(recipe, spinner, ingredients)
		hostcall("game.craft_make", recipe, spinner, ingredients or {})
	end,
}

-- A trade: `trade.ask("open", partner)`, `trade.ask("accept")`, `"decline"`, `"close"`.
api.trade = {
	ask = function(what, partner)
		hostcall("game.trade", what, partner)
	end,
}
