-- `dereth.ubscript.actions`: UtilityBelt's actions over the plugin interface's. A script's action
-- (`game.Actions.ObjectUse(id)`, `ObjectUseAction.new(id)`) waits its turn in UtilityBelt's
-- queue: the highest priority first, an immediate action at once, one at a time otherwise. Its
-- turn come, it is carried out with the plugin interface's actions, which wait their own turn in
-- the host's queue and are held to the world's policy. A failure the queue may retry (a timeout,
-- a busy character) is tried again up to the action's `MaxRetryCount`; the action then ends with
-- `Success`, `Error` (an `ActionError`) and `ErrorDetails`, its callback runs, and whatever awaits
-- it carries on.

return function(U)
	local D, class, state, norm, list = U.D, U.class, U.state, U.norm, U.list
	local ev, num, id_of = U.ev, U.num, U.id_of

	-- Options ------------------------------------------------------------------------------------

	local ActionOptions = class("ActionOptions")
	local OPTION_FIELDS = { "Name", "Priority", "SkipChecks", "MaxRetryCount", "TimeoutMilliseconds" }
	do
		local get, set = {}, {}
		for _, f in ipairs(OPTION_FIELDS) do
			local n = norm(f)
			get[f] = function(o)
				return state(o)[n]
			end
			set[f] = function(o, v)
				state(o)[n] = v
			end
		end
		ActionOptions.define({
			get = get,
			set = set,
			call = {
				ToString = function(o)
					local s = state(o)
					return string.format(
						"ActionOptions(Priority=%s, MaxRetryCount=%s, TimeoutMilliseconds=%s)",
						tostring(s.priority),
						tostring(s.maxretrycount),
						tostring(s.timeoutmilliseconds)
					)
				end,
			},
		})
	end
	local function options_of(o)
		if o == nil then
			return ActionOptions.new({})
		end
		if U.is(o, ActionOptions) then
			return o
		end
		if type(o) == "table" then
			local s = {}
			for k, v in pairs(o) do
				if type(k) == "string" then
					s[norm(k)] = v
				end
			end
			return ActionOptions.new(s)
		end
		error("an action's options are ActionOptions or nil", 3)
	end
	U.globals.ActionOptions = U.static("ActionOptions", {
		new = function(t)
			return options_of(t)
		end,
	})

	-- The actions --------------------------------------------------------------------------------

	local QueueAction = class("QueueAction")
	U.QueueAction = QueueAction

	-- ActionType numbers: the queue's default priorities.
	local T = {
		Immediate = 2147483647,
		Wield = 20000,
		LoginLogoff = 16000,
		Vendor = 15500,
		Trade = 15000,
		Combat = 14000,
		CastSpell = 13000,
		Navigation = 12000,
		Fellow = 11000,
		Allegiance = 10000,
		Inventory = 9000,
		Misc = 8000,
	}

	-- Each action: its type, its arguments' names (the fields a script reads back), their
	-- defaults, and what it does (inside a coroutine of the queue's: nil for done, or an
	-- `ActionError` name and the details).
	local SPECS = {}
	U.ACTION_SPECS = SPECS

	local function spec(name, t)
		t.name = name
		SPECS[name] = t
	end

	-- Why a plugin-interface action ended badly, as UtilityBelt's error and whether the queue may
	-- try again.
	local function failure(reason)
		local r = tostring(reason)
		if r == "timeout" then
			return "TimedOut", r, true
		elseif r == "busy" then
			return "TooBusy", r, true
		elseif r == "stopped" or r == "replaced" or r == "interrupted" then
			return "Cancelled", r, false
		elseif r == "target_gone" then
			return "InvalidTargetObject", r, false
		elseif r == "not_in_magic_mode" then
			return "InvalidCombatMode", r, false
		elseif string.find(r, "^refused by the client") then
			return "PreconditionFailed", r, false
		elseif string.find(r, "^denied_by_world") then
			return "PreconditionFailed", r, false
		end
		local code = string.match(r, "^%a+ %((0x%x+)%)")
		if code ~= nil then
			local n = tonumber(code)
			local name = U.enum_name("WeenieError", n)
			-- The server's "you are too busy" passes: the queue tries again.
			if n == 0x1D then
				return "TooBusy", name, true
			end
			return "ServerError", name ~= tostring(n) and name or r, false
		end
		return "ServerError", r, false
	end
	U.failure = failure

	-- The queue's actions answer a plugin-interface promise: nil once it completed, else the
	-- error. A script's own timeout (`TimeoutMilliseconds`), when it set one, ends the wait.
	local function settle(a, p)
		local s = a ~= nil and state(a) or {}
		local limit = s.options and state(s.options).timeoutmilliseconds
		if limit ~= nil then
			local deadline = U.now() + num(limit) / 1000
			while p.state == "pending" and U.now() < deadline do
				D.task.sleep(50)
			end
			if p.state == "pending" then
				return "TimedOut", "timed out after " .. tostring(num(limit)) .. " ms", true
			end
		else
			D.pawait(p)
		end
		if p.state == "completed" then
			s.value = p.value
			return nil
		end
		return failure(p.reason)
	end
	U.settle = settle

	-- A plugin-interface call that may raise (bad arguments, a host without the capability): the
	-- error as the action's.
	local function call(a, f, ...)
		local ok, p = pcall(f, ...)
		if not ok then
			return "Exception", tostring(p), false
		end
		if type(p) == "table" and p.state ~= nil then
			return settle(a, p)
		end
		return nil
	end
	U.call = call

	local function f(a, k)
		return state(a).fields[k]
	end

	local function me()
		return D.game.player.id
	end

	local function refuse(what, why)
		return function()
			return "Exception", U.refusal(what, why), false
		end
	end

	spec("BlankAction", { type = T.Misc, run = function() end })
	spec("SleepAction", {
		type = T.Immediate,
		args = { "Milliseconds" },
		run = function(a)
			U.sleep(num(f(a, "milliseconds")) or 0)
		end,
	})
	spec("RunAllOrderedAction", {
		type = T.Misc,
		args = { "Actions" },
		run = function(a)
			for _, child in ipairs(f(a, "actions") or {}) do
				U.execute(child)
				local cs = state(child)
				if cs.error ~= "None" then
					return cs.error, cs.details, false
				end
			end
		end,
	})
	spec("LoginAction", {
		type = T.LoginLogoff,
		args = { "ObjectId" },
		run = function(a)
			return call(a, D.session.select_character, num(f(a, "objectid")))
		end,
	})
	spec("LogoutAction", {
		type = T.LoginLogoff,
		run = function(a)
			return call(a, D.session.logout)
		end,
	})
	-- Spending experience: the plugin interface raises by one at the raise's own cost, and only
	-- when the player asked.
	for _, k in ipairs({
		{ "AttributeAddExperienceAction", "Attribute", "raise_attribute" },
		{ "SkillAddExperienceAction", "Skill", "raise_skill" },
		{ "VitalAddExperienceAction", "Vital", "raise_vital" },
	}) do
		spec(k[1], {
			type = T.Immediate,
			args = { k[2], "ExperienceToSpend" },
			run = function(a)
				return call(a, D.character[k[3]], num(f(a, norm(k[2]))))
			end,
		})
	end
	spec("SkillAdvanceAction", {
		type = T.Immediate,
		args = { "Skill", "CreditsToSpend" },
		run = refuse("SkillAdvance", "the plugin interface does not train skills"),
	})
	spec("ObjectWieldAction", {
		type = T.Wield,
		args = { "ObjectId", "Slot" },
		defaults = { slot = 0 },
		run = function(a)
			return call(a, D.items.wield, id_of(f(a, "objectid")))
		end,
	})

	-- Casting needs magic stance: the character enters it first, as UtilityBelt's cast does.
	local function into_magic(a)
		if (D.game.player.combat_mode or 1) == 8 then
			return nil
		end
		local e, d, again = call(a, D.combat.set_mode, "combat")
		if e ~= nil then
			return e, d, again
		end
		if (D.game.player.combat_mode or 1) ~= 8 then
			return "InvalidCombatMode", "no caster is wielded", false
		end
	end
	spec("CastSpellAction", {
		type = T.CastSpell,
		args = { "SpellId", "TargetId" },
		defaults = { targetid = 0 },
		run = function(a)
			local e, d, again = into_magic(a)
			if e ~= nil then
				return e, d, again
			end
			local target = num(f(a, "targetid"))
			if target == 0 then
				target = nil
			end
			return call(a, D.magic.cast, num(f(a, "spellid")), target)
		end,
	})
	spec("CastEquippedWandSpellAction", {
		type = T.CastSpell,
		args = { "TargetId" },
		defaults = { targetid = 0 },
		run = function(a)
			local caster
			for _, h in ipairs(D.game.world:find({ wielder = me(), class = 31 })) do
				caster = h.id
			end
			if caster == nil then
				return "InvalidSourceObject", "no caster is wielded", false
			end
			local target = num(f(a, "targetid"))
			if target == 0 then
				target = me()
			end
			return call(a, D.act.use_on, caster, target)
		end,
	})
	spec("SetCombatModeAction", {
		type = T.Combat,
		args = { "CombatMode" },
		run = function(a)
			local want = num(f(a, "combatmode")) or 1
			local e, d, again = call(a, D.combat.set_mode, want == 1 and "peace" or "combat")
			if e ~= nil then
				return e, d, again
			end
			local now = D.game.player.combat_mode or 1
			if want ~= 1 and now ~= want then
				return "InvalidCombatMode", "the character is in " .. U.enum_name("CombatMode", now), false
			end
		end,
	})
	spec("ObjectDropAction", {
		type = T.Inventory,
		args = { "ObjectId" },
		run = function(a)
			return call(a, D.items.drop, id_of(f(a, "objectid")))
		end,
	})
	spec("FellowCreateAction", {
		type = T.Fellow,
		args = { "FellowshipName", "ShareExperience" },
		defaults = { shareexperience = true },
		run = function(a)
			return call(a, D.social.fellowship_create, tostring(f(a, "fellowshipname")), f(a, "shareexperience") ~= false)
		end,
	})
	spec("FellowDisbandAction", {
		type = T.Fellow,
		run = function(a)
			return call(a, D.social.fellowship_quit, true)
		end,
	})
	spec("FellowQuitAction", {
		type = T.Fellow,
		args = { "Disband" },
		defaults = { disband = false },
		run = function(a)
			return call(a, D.social.fellowship_quit, f(a, "disband") == true)
		end,
	})
	for _, k in ipairs({
		{ "FellowDismissAction", "fellowship_dismiss" },
		{ "FellowRecruitAction", "fellowship_recruit" },
		{ "FellowSetLeaderAction", "fellowship_leader" },
	}) do
		spec(k[1], {
			type = T.Fellow,
			args = { "ObjectId" },
			run = function(a)
				return call(a, D.social[k[2]], id_of(f(a, "objectid")))
			end,
		})
	end
	spec("FellowSetOpenAction", {
		type = T.Fellow,
		args = { "Open" },
		run = function(a)
			local fs = D.game.player.fellowship
			if fs == nil then
				return "NotInFellow", "the character is in no fellowship", false
			end
			if (fs.open == true) == (f(a, "open") == true) then
				return nil
			end
			return call(a, D.social.fellowship_openness)
		end,
	})
	spec("ObjectGiveAction", {
		type = T.Inventory,
		args = { "ObjectId", "TargetId" },
		run = function(a)
			return call(a, D.items.give, id_of(f(a, "objectid")), id_of(f(a, "targetid")))
		end,
	})
	spec("InvokeChatAction", {
		type = T.Immediate,
		args = { "Text" },
		run = function(a)
			return U.invoke_chat(a, tostring(f(a, "text") or ""))
		end,
	})
	spec("ObjectMoveAction", {
		type = T.Inventory,
		args = { "ObjectId", "TargetId", "Slot", "Stack" },
		defaults = { slot = 0, stack = true },
		run = function(a)
			local id, target = id_of(f(a, "objectid")), id_of(f(a, "targetid"))
			local item = D.game.world:get(id)
			-- A stack goes onto one of its kind in the container, as UtilityBelt's move does.
			if f(a, "stack") ~= false and item ~= nil and (item.max_stack or 1) > 1 then
				for _, h in ipairs(D.game.world:find({ container = target, name = item.name })) do
					if h.id ~= id and (h.stack or 1) + (item.stack or 1) <= (h.max_stack or 1) then
						return call(a, D.items.merge, id, h.id)
					end
				end
			end
			return call(a, D.items.move, id, target, (num(f(a, "slot")) or 0) + 1)
		end,
	})
	spec("ObjectSplitAction", {
		type = T.Inventory,
		args = { "ObjectId", "TargetId", "NewStackSize", "Slot" },
		defaults = { slot = 0 },
		run = function(a)
			local before = {}
			local target = id_of(f(a, "targetid"))
			for _, h in ipairs(D.game.world:find({ container = target })) do
				before[h.id] = true
			end
			local e, d, again = call(a, D.items.split, id_of(f(a, "objectid")), num(f(a, "newstacksize")), target)
			if e == nil then
				for _, h in ipairs(D.game.world:find({ container = target })) do
					if not before[h.id] then
						state(a).fields.newstackobjectid = h.id
					end
				end
			end
			return e, d, again
		end,
	})
	spec("ObjectAppraiseAction", {
		type = T.Immediate,
		args = { "ObjectId" },
		run = function(a)
			return call(a, D.act.appraise, id_of(f(a, "objectid")))
		end,
	})
	spec("SalvageAddAction", {
		type = T.Immediate,
		args = { "ObjectId" },
		run = function(a)
			table.insert(U.salvage_list, id_of(f(a, "objectid")))
		end,
	})
	spec("SalvageAction", {
		type = T.Immediate,
		args = { "ObjectIds" },
		run = function(a)
			local ids = U.salvage_list
			U.salvage_list = {}
			state(a).fields.objectids = list(table.clone(ids))
			if #ids == 0 then
				return "NoActions", "nothing was added to salvage", false
			end
			return call(a, D.items.salvage, ids)
		end,
	})
	spec("ObjectSelectAction", {
		type = T.Immediate,
		args = { "ObjectId" },
		run = function(a)
			return call(a, D.act.select, id_of(f(a, "objectid")))
		end,
	})
	spec("ObjectUseAction", {
		type = T.Inventory,
		args = { "ObjectId", "TargetId" },
		defaults = { targetid = 0 },
		run = function(a)
			local id, target = id_of(f(a, "objectid")), num(id_of(f(a, "targetid"))) or 0
			if target == 0 then
				return call(a, D.act.use, id)
			end
			return call(a, D.act.use_on, id, target)
		end,
	})
	spec("ObjectApplyAction", {
		type = T.Inventory,
		args = { "ObjectId", "TargetId" },
		run = function(a)
			return call(a, D.act.use_on, id_of(f(a, "objectid")), id_of(f(a, "targetid")))
		end,
	})
	spec("SetAutorunAction", {
		type = T.Navigation,
		args = { "Enabled" },
		run = function(a)
			return call(a, D.nav.autorun, f(a, "enabled") == true)
		end,
	})
	for _, k in ipairs({
		{ "TradeAcceptAction", "accept" },
		{ "TradeDeclineAction", "decline" },
		{ "TradeEndAction", "close" },
		{ "TradeResetAction", "reset" },
	}) do
		spec(k[1], {
			type = T.Trade,
			run = function(a)
				return call(a, D.trade[k[2]])
			end,
		})
	end
	spec("TradeAddAction", {
		type = T.Trade,
		args = { "ObjectId" },
		run = function(a)
			return call(a, D.trade.add, id_of(f(a, "objectid")))
		end,
	})
	spec("AllegianceSwearAction", {
		type = T.Allegiance,
		args = { "ObjectId" },
		run = refuse("AllegianceSwear", "the plugin interface has no allegiance actions"),
	})
	spec("AllegianceBreakAction", {
		type = T.Allegiance,
		args = { "ObjectId" },
		run = refuse("AllegianceBreak", "the plugin interface has no allegiance actions"),
	})

	-- The vendor's buy and sell lists, which UtilityBelt fills before it buys or sells them.
	U.buy_list, U.sell_list, U.salvage_list = {}, {}, {}
	local function vendor_open()
		return D.vendor.info() ~= nil
	end
	spec("VendorAddToBuyListAction", {
		type = T.Vendor,
		args = { "ObjectId", "Amount" },
		defaults = { amount = 1 },
		run = function(a)
			if not vendor_open() then
				return "VendorNotOpen", "no vendor is open", false
			end
			table.insert(U.buy_list, { id = id_of(f(a, "objectid")), count = num(f(a, "amount")) or 1 })
		end,
	})
	spec("VendorAddToSellListAction", {
		type = T.Vendor,
		args = { "ObjectId" },
		run = function(a)
			if not vendor_open() then
				return "VendorNotOpen", "no vendor is open", false
			end
			table.insert(U.sell_list, id_of(f(a, "objectid")))
		end,
	})
	spec("VendorBuyAllAction", {
		type = T.Vendor,
		run = function(a)
			local items = U.buy_list
			U.buy_list = {}
			if #items == 0 then
				return "NoActions", "the buy list is empty", false
			end
			return call(a, D.vendor.buy, items)
		end,
	})
	spec("VendorSellAllAction", {
		type = T.Vendor,
		run = function(a)
			local ids = U.sell_list
			U.sell_list = {}
			if #ids == 0 then
				return "NoActions", "the sell list is empty", false
			end
			return call(a, D.vendor.sell, ids)
		end,
	})
	spec("VendorClearBuyListAction", {
		type = T.Vendor,
		run = function()
			U.buy_list = {}
		end,
	})
	spec("VendorClearSellListAction", {
		type = T.Vendor,
		run = function()
			U.sell_list = {}
		end,
	})
	spec("VendorBuyAction", {
		type = T.Vendor,
		args = { "ObjectId", "Amount" },
		defaults = { amount = 1 },
		run = function(a)
			return call(a, D.vendor.buy, { { id = id_of(f(a, "objectid")), count = num(f(a, "amount")) or 1 } })
		end,
	})
	spec("VendorSellAction", {
		type = T.Vendor,
		args = { "ObjectId", "Amount" },
		defaults = { amount = 1 },
		run = function(a)
			return call(a, D.vendor.sell, { id_of(f(a, "objectid")) })
		end,
	})
	-- A tell to an object by id: the plugin interface tells by name, so this reaches players only.
	spec("SendTellByIdAction", {
		type = T.Immediate,
		args = { "ObjectId", "Message" },
		run = function(a)
			local h = D.game.world:get(id_of(f(a, "objectid")))
			if h == nil then
				return "InvalidTargetObject", "the client does not know that object", false
			end
			if h.class ~= "Player" then
				return "InvalidTargetObject",
					U.refusal("SendTellById to " .. tostring(h.name), "a tell goes by name, and only to a player"),
					false
			end
			return U.invoke_chat(a, "/tell " .. h.name .. ", " .. tostring(f(a, "message") or ""))
		end,
	})
	spec("ObjectInscribeAction", {
		type = T.Immediate,
		args = { "ObjectId", "Inscription" },
		run = function(a)
			return call(a, D.items.inscribe, id_of(f(a, "objectid")), tostring(f(a, "inscription") or ""))
		end,
	})

	-- Making an action ----------------------------------------------------------------------------

	local function new_action(name, ...)
		local sp = SPECS[name]
		assert(sp ~= nil, "no action " .. tostring(name))
		local args = table.pack(...)
		local fields = {}
		local n = 0
		for i, field in ipairs(sp.args or {}) do
			n = i
			local v = args[i]
			if v == nil and sp.defaults ~= nil then
				v = sp.defaults[norm(field)]
			end
			fields[norm(field)] = v
		end
		local options = options_of(args[n + 1])
		local s = {
			spec = sp,
			fields = fields,
			options = options,
			started = false,
			finished = false,
			error = "None",
			details = "",
			retries = 0,
			callbacks = {},
			waiters = {},
		}
		local a = QueueAction.new(s)
		s.on_started = U.event(name .. ".OnStarted")
		s.on_finished = U.event(name .. ".OnFinished")
		return a
	end
	U.new_action = new_action

	-- The action classes as globals: `ObjectUseAction.new(id, target, options)`.
	local class_statics = {}
	function U.action_class(name)
		if SPECS[name] == nil then
			return nil
		end
		local s = class_statics[name]
		if s == nil then
			s = U.static(name, {
				new = function(...)
					return new_action(name, ...)
				end,
			})
			class_statics[name] = s
		end
		return s
	end

	local function priority(a)
		local s = state(a)
		local p = state(s.options).priority
		if p ~= nil then
			return num(p)
		end
		return s.spec.type
	end

	QueueAction.define({
		get = {
			Priority = priority,
			Name = function(a)
				local s = state(a)
				return state(s.options).name or s.spec.name
			end,
			Options = function(a)
				return state(a).options
			end,
			Timeout = function(a)
				local t = state(state(a).options).timeoutmilliseconds
				return U.TimeSpan((num(t) or 5000) / 1000)
			end,
			StartedAt = function(a)
				return U.DateTime(state(a).started_at or 0)
			end,
			FinishedAt = function(a)
				return U.DateTime(state(a).finished_at or 0)
			end,
			IsStarted = function(a)
				return state(a).started
			end,
			IsRunning = function(a)
				return state(a).started and not state(a).finished
			end,
			IsFinished = function(a)
				return state(a).finished
			end,
			IsRunningPreconditions = function()
				return false
			end,
			CurrentRetryCount = function(a)
				return state(a).retries
			end,
			ActionType = function(a)
				return ev("ActionType", state(a).spec.type)
			end,
			Success = function(a)
				return state(a).error == "None"
			end,
			Error = function(a)
				return U.enum("ActionError")[state(a).error]
			end,
			ErrorDetails = function(a)
				return state(a).details
			end,
			Preconditions = function()
				return list({})
			end,
			RanPreconditions = function()
				return list({})
			end,
			OnStarted = function(a)
				return state(a).on_started
			end,
			OnFinished = function(a)
				return state(a).on_finished
			end,
		},
		call = {
			Await = function(a)
				return U.await(a)
			end,
			IsValid = function()
				return true
			end,
			SetPermanentResult = function(a, err, text)
				U.finish(a, U.enum_name("ActionError", num(err)), text)
			end,
			SetTemporaryResult = function(a, err, text)
				U.finish(a, U.enum_name("ActionError", num(err)), text)
			end,
			Update = function() end,
			ToString = function(a)
				return state(a).spec.name
			end,
		},
		refused = {
			SetPermanentResultAfter = "the queue sets an action's result when it ends",
			SetTemporaryResultAfter = "the queue sets an action's result when it ends",
		},
	})
	QueueAction.tostring = function(a)
		return state(a).spec.name
	end
	-- An action's own fields: `ObjectId`, `TargetId`, `SpellId`, `FellowshipName`, ...
	QueueAction.fallback = function(a, n)
		local fields = state(a).fields
		if fields[n] ~= nil then
			return true, fields[n]
		end
		local sp = state(a).spec
		for _, field in ipairs(sp.args or {}) do
			if norm(field) == n then
				return true, nil
			end
		end
		if n == "newstackobjectid" or n == "summonedportalid" then
			return true, fields[n] or 0
		end
		if n == "useactiontype" then
			return true, ev("UseActionType", 0)
		end
		return false
	end

	-- The queue ----------------------------------------------------------------------------------

	local queue = {} -- waiting, in the order added
	local immediate = {} -- running at once, beside the queue
	local current = nil -- the queued action being carried out

	-- End an action: its result, its callback and whatever waits for it.
	local function finish(a, err, details)
		local s = state(a)
		if s.finished then
			return
		end
		s.finished = true
		s.finished_at = U.now()
		s.error = err or "None"
		s.details = details or (err ~= nil and err ~= "None" and err or "")
		for _, cb in ipairs(s.callbacks) do
			U.spawn(cb, a)
		end
		U.fire(s.on_finished, a)
		for _, co in ipairs(s.waiters) do
			U.resume(co, a)
		end
		s.waiters = {}
	end
	U.finish = finish

	-- Carry out an action now, retrying what may be retried, inside a coroutine.
	local function execute(a)
		local s = state(a)
		if s.finished then
			return
		end
		s.started = true
		s.started_at = U.now()
		U.fire(s.on_started, a)
		local max = num(state(s.options).maxretrycount) or 5
		while true do
			local ok, err, details, again = pcall(s.spec.run, a)
			if not ok then
				err, details, again = "Exception", tostring(err), false
			end
			if s.finished then
				return
			end
			if err == nil then
				finish(a, "None", "")
				return
			end
			if again and s.retries < max then
				s.retries = s.retries + 1
			else
				finish(a, err, details)
				return
			end
		end
	end
	U.execute = execute

	local pump
	local function run_current(a)
		current = a
		local ok = U.spawn(function()
			execute(a)
			if current == a then
				current = nil
			end
			pump()
		end)
		if not ok and current == a then
			current = nil
		end
	end
	pump = function()
		if current ~= nil then
			return
		end
		local best, at = nil, nil
		for i, a in ipairs(queue) do
			if best == nil or priority(a) > priority(best) then
				best, at = a, i
			end
		end
		if best ~= nil then
			table.remove(queue, at)
			run_current(best)
		end
	end

	-- Add an action to the queue: an immediate one starts at once.
	local function add(a)
		local s = state(a)
		if s.queued or s.finished then
			return false
		end
		s.queued = true
		if s.spec.type == T.Immediate and state(s.options).priority == nil then
			table.insert(immediate, a)
			U.spawn(function()
				execute(a)
				for i, x in ipairs(immediate) do
					if x == a then
						table.remove(immediate, i)
						break
					end
				end
			end)
			return true
		end
		table.insert(queue, a)
		pump()
		return true
	end
	U.queue_add = add

	-- The queue goes with the script: what waits is dropped, unfinished.
	function U.queue_clear()
		queue = {}
		immediate = {}
		current = nil
		U.buy_list, U.sell_list, U.salvage_list = {}, {}, {}
	end

	local ActionQueue = class("ActionQueue")
	ActionQueue.define({
		get = {
			Queue = function()
				local out = table.clone(queue)
				if current ~= nil then
					table.insert(out, 1, current)
				end
				return list(out)
			end,
			ImmediateQueue = function()
				return list(table.clone(immediate))
			end,
		},
		call = {
			Add = function(_, a)
				return add(a)
			end,
			Remove = function(_, a)
				for i, x in ipairs(queue) do
					if x == a then
						table.remove(queue, i)
						state(a).queued = false
						return
					end
				end
			end,
			Contains = function(_, a)
				if a == current then
					return true
				end
				for _, x in ipairs(queue) do
					if x == a then
						return true
					end
				end
				for _, x in ipairs(immediate) do
					if x == a then
						return true
					end
				end
				return false
			end,
			Clear = function()
				for _, a in ipairs(queue) do
					state(a).queued = false
				end
				queue = {}
			end,
			Dispose = function() end,
		},
	})
	U.action_queue = ActionQueue.new({})

	-- `await(action)`: inside a script coroutine, wait until the action ends (queueing it if it is
	-- not), and answer it.
	function U.await(a)
		if not U.is(a, QueueAction) then
			if type(a) == "table" and a.state ~= nil and a.next ~= nil then
				-- A plugin-interface promise, from a script that mixes the two.
				D.pawait(a)
				return a
			end
			error("await needs an action", 2)
		end
		local s = state(a)
		if not s.queued and not s.finished then
			add(a)
		end
		if s.finished then
			return a
		end
		if not coroutine.isyieldable() then
			error("await can only be called from a script's own code, which runs as a coroutine", 2)
		end
		table.insert(s.waiters, (coroutine.running()))
		while not s.finished do
			coroutine.yield()
		end
		return a
	end

	-- `game.Actions`: each action made, queued and answered; the callback is the last argument.
	local CLIENT = {
		Blank = "BlankAction",
		Login = "LoginAction",
		Logout = "LogoutAction",
		AttributeAddExperience = "AttributeAddExperienceAction",
		SkillAddExperience = "SkillAddExperienceAction",
		VitalAddExperience = "VitalAddExperienceAction",
		SkillAdvance = "SkillAdvanceAction",
		ObjectWield = "ObjectWieldAction",
		CastSpell = "CastSpellAction",
		CastEquippedWandSpell = "CastEquippedWandSpellAction",
		SetCombatMode = "SetCombatModeAction",
		ObjectDrop = "ObjectDropAction",
		FellowCreate = "FellowCreateAction",
		FellowDisband = "FellowDisbandAction",
		FellowDismiss = "FellowDismissAction",
		FellowSetLeader = "FellowSetLeaderAction",
		FellowQuit = "FellowQuitAction",
		FellowRecruit = "FellowRecruitAction",
		FellowSetOpen = "FellowSetOpenAction",
		ObjectGive = "ObjectGiveAction",
		InvokeChat = "InvokeChatAction",
		ObjectMove = "ObjectMoveAction",
		ObjectSplit = "ObjectSplitAction",
		ObjectAppraise = "ObjectAppraiseAction",
		SalvageAdd = "SalvageAddAction",
		Salvage = "SalvageAction",
		ObjectSelect = "ObjectSelectAction",
		ObjectUse = "ObjectUseAction",
		SetAutorun = "SetAutorunAction",
		TradeAccept = "TradeAcceptAction",
		TradeAdd = "TradeAddAction",
		TradeDecline = "TradeDeclineAction",
		TradeEnd = "TradeEndAction",
		TradeReset = "TradeResetAction",
		AllegianceSwear = "AllegianceSwearAction",
		AllegianceBreak = "AllegianceBreakAction",
		VendorAddToBuyList = "VendorAddToBuyListAction",
		VendorAddToSellList = "VendorAddToSellListAction",
		VendorBuyAll = "VendorBuyAllAction",
		VendorSellAll = "VendorSellAllAction",
		VendorClearBuyList = "VendorClearBuyListAction",
		VendorClearSellList = "VendorClearSellListAction",
		SendTellById = "SendTellByIdAction",
		Sleep = "SleepAction",
		Inscribe = "ObjectInscribeAction",
	}
	U.CLIENT_ACTIONS = CLIENT

	-- Make, queue and answer one action; `args` are the action's arguments, then its options and
	-- its callback.
	local function client_action(name, ...)
		local sp = SPECS[name]
		local nargs = #(sp.args or {})
		local args = table.pack(...)
		-- `Salvage()` takes no list: the action fills it from what was added.
		if name == "SalvageAction" then
			nargs = 0
		end
		local ctor = {}
		for i = 1, nargs do
			ctor[i] = args[i]
		end
		ctor[nargs + 1] = args[nargs + 1]
		local cb = args[nargs + 2]
		local a = new_action(name, table.unpack(ctor, 1, nargs + 1))
		if type(cb) == "function" then
			table.insert(state(a).callbacks, cb)
		end
		add(a)
		return a
	end

	local Actions = {}
	for method, name in pairs(CLIENT) do
		Actions[method] = function(...)
			return client_action(name, ...)
		end
	end
	-- `RunAllOrdered(actions, options, callback)`: the actions in turn, as one.
	Actions.RunAllOrdered = function(actions, options, cb)
		local l = {}
		for _, x in ipairs(actions or {}) do
			table.insert(l, x)
		end
		local a = new_action("RunAllOrderedAction", list(l), options)
		if type(cb) == "function" then
			table.insert(state(a).callbacks, cb)
		end
		add(a)
		return a
	end
	U.Actions = Actions

	local ClientActions = class("ClientActions")
	local calls = {}
	for method, fn in pairs(Actions) do
		calls[method] = function(_, ...)
			return fn(...)
		end
	end
	calls.Dispose = function() end
	ClientActions.define({ call = calls })
	U.client_actions = ClientActions.new({})

	-- Chat lines: `InvokeChat`. A line naming another plugin's command (`/vt`, `/ub`) goes to the
	-- part of this library that answers it; the rest are typed, waiting while the plugin interface's
	-- pace for chat (one line a second, three at once) holds them back.
	function U.invoke_chat(a, text)
		text = string.gsub(text, "^%s+", "")
		local routed, err, details = U.route_chat(text)
		if routed then
			return err, details, false
		end
		local deadline = U.now() + 15
		while true do
			local ok, why = pcall(function()
				if string.sub(text, 1, 1) == "/" or string.sub(text, 1, 1) == "@" then
					D.chat.command(text)
				else
					D.chat.say(text)
				end
			end)
			if ok then
				return nil
			end
			why = tostring(why)
			if string.find(why, "one chat line a second", 1, true) and U.now() < deadline then
				D.task.sleep(250)
			elseif string.find(why, "a plugin's command", 1, true) then
				return "UnknownCommand", why, false
			else
				return "Exception", why, false
			end
		end
	end

	-- Overridden by the parts that answer other plugins' commands.
	function U.route_chat()
		return false
	end
end
