-- `dereth.ubscript.world`: UtilityBelt's `game` and what hangs off it (the world, the character,
-- its fellowship, allegiance, trade, spell book and enchantments, the vendor), read from the
-- plugin interface each time a script asks, and its events, heard from the plugin interface's.

return function(U)
	local D, class, state, norm, list, dict = U.D, U.class, U.state, U.norm, U.list, U.dict
	local ev, num, id_of, wo, event, fire, listen = U.ev, U.num, U.id_of, U.wo, U.event, U.fire, U.listen

	-- Event arguments ------------------------------------------------------------------------------

	-- An event's argument: its fields by name; `Eat` (and a confirmation's answers) may be set.
	local EventArgs = class("EventArgs")
	EventArgs.fallback = function(a, n)
		local f = state(a).fields
		if f[n] ~= nil then
			return true, f[n]
		end
		if state(a).names[n] then
			return true, nil
		end
		return false
	end
	EventArgs.define({
		set = {
			Eat = function(a, v)
				local st = state(a)
				st.fields.eat = v == true
				if st.on_eat ~= nil then
					st.on_eat(v == true)
				end
			end,
			ClickYes = function()
				error(U.refusal("ClickYes", "the plugin interface does not answer the game's confirmations"), 2)
			end,
			ClickNo = function()
				error(U.refusal("ClickNo", "the plugin interface does not answer the game's confirmations"), 2)
			end,
		},
	})
	local function args(name, fields)
		local f, names = {}, {}
		for k, v in pairs(fields) do
			f[norm(k)] = v
			names[norm(k)] = true
		end
		local a = EventArgs.new({ fields = f, names = names })
		state(a).name = name
		return a
	end
	U.args = args
	EventArgs.tostring = function(a)
		return state(a).name or "EventArgs"
	end

	-- A script's object events: `obj.OnPositionChanged`, `obj.OnDestroyed`.
	local object_events = {}
	function U.object_event(id, kind)
		local key = kind .. ":" .. tostring(id)
		local e = object_events[key]
		if e ~= nil then
			return e
		end
		if kind == "moved" then
			e = event("OnPositionChanged", function()
				listen("world.object_moved", function(m)
					if m.id == id then
						fire(e, args("ServerPositionChangedEventArgs", { Weenie = wo(id), Position = U.position(m.position) }))
					end
				end)
			end)
		else
			e = event("OnDestroyed", function()
				listen("world.object_removed", function(m)
					if m.id == id then
						fire(e, args("ObjectReleasedEventArgs", { ObjectId = id }))
					end
				end)
			end)
		end
		object_events[key] = e
		U.at_end(function()
			object_events[key] = nil
		end)
		return e
	end

	-- When the client first knew an object, and when its last appraisal arrived.
	local created, appraised = {}, {}
	function U.created_at(id)
		return created[id]
	end
	function U.appraised_at(id)
		return appraised[id]
	end

	-- Spells -----------------------------------------------------------------------------------------

	local MAGIC_SKILL = { [1] = 34, [2] = 33, [3] = 32, [4] = 31, [5] = 43 }
	local function skill_current(id)
		for _, s in ipairs(D.game.player.skills or {}) do
			if s.id == id then
				return s.current or 0
			end
		end
		return 0
	end
	local function magic_skill(school)
		local id = MAGIC_SKILL[num(school)]
		if id == nil then
			return 0
		end
		return skill_current(id)
	end
	U.magic_skill = magic_skill

	local known_cache, known_at = nil, nil
	local function known_ids()
		local t = D.os.clock()
		if known_cache == nil or known_at ~= t then
			known_cache = {}
			for _, s in ipairs(D.magic.known() or {}) do
				known_cache[s.id] = true
			end
			known_at = t
		end
		return known_cache
	end

	local Spell = class("Spell")
	U.Spell = Spell
	local function spell(id)
		local s = D.magic.spell(num(id))
		if s == nil then
			return nil
		end
		return Spell.new({ s = s })
	end
	U.spell = spell
	Spell.define({
		get = {
			Id = function(sp)
				return state(sp).s.id
			end,
			Name = function(sp)
				return state(sp).s.name
			end,
			Description = function(sp)
				return state(sp).s.description or ""
			end,
			School = function(sp)
				return ev("MagicSchool", state(sp).s.school or 0)
			end,
			Category = function(sp)
				return state(sp).s.category or 0
			end,
			Level = function(sp)
				return state(sp).s.level or 0
			end,
			Power = function(sp)
				return state(sp).s.difficulty or 0
			end,
			BaseMana = function(sp)
				return state(sp).s.mana or 0
			end,
			Duration = function(sp)
				return state(sp).s.duration or -1
			end,
			Icon = function(sp)
				return state(sp).s.icon or 0
			end,
		},
		call = {
			IsKnown = function(sp)
				return known_ids()[state(sp).s.id] == true
			end,
			HasSkill = function(sp, modifier)
				local s = state(sp).s
				return magic_skill(s.school or 0) >= (s.difficulty or 0) + (num(modifier) or 20)
			end,
			HasComponents = function()
				return true
			end,
			HasFociOrInfusedAugmentation = function()
				return true
			end,
			ToString = function(sp)
				return state(sp).s.name
			end,
		},
		refused = {
			Components = "the plugin interface does not read spell formulas",
			OldSchoolComponents = "the plugin interface does not read spell formulas",
			FociComponents = "the plugin interface does not read spell formulas",
			Formula = "the plugin interface does not read spell formulas",
			StatModType = "the plugin interface does not read a spell's effect",
			StatModKey = "the plugin interface does not read a spell's effect",
			StatModVal = "the plugin interface does not read a spell's effect",
		},
	})
	Spell.tostring = function(sp)
		return state(sp).s.name or "Spell"
	end
	-- The spell table as the plugin interface reads it: a spell's name, icon, school, level,
	-- category, description, difficulty, mana and duration, and not the rest.
	local SPELL_ONLY = "the plugin interface reads a spell's name, icon, school, level, category, difficulty, mana and duration"
	Spell.define({
		refused = {
			Flags = SPELL_ONLY,
			BaseRangeConstant = SPELL_ONLY,
			BaseRangeMod = SPELL_ONLY,
			SpellEconomyMod = SPELL_ONLY,
			FormulaVersion = SPELL_ONLY,
			ComponentLoss = SPELL_ONLY,
			MetaSpellType = SPELL_ONLY,
			MetaSpellId = SPELL_ONLY,
			DegradeModifier = SPELL_ONLY,
			DegradeLimit = SPELL_ONLY,
			PortalLifetime = SPELL_ONLY,
			CasterEffect = SPELL_ONLY,
			TargetEffect = SPELL_ONLY,
			FizzleEffect = SPELL_ONLY,
			RecoveryInterval = SPELL_ONLY,
			RecoveryAmount = SPELL_ONLY,
			DisplayOrder = SPELL_ONLY,
			NonComponentTargetType = SPELL_ONLY,
			ManaMod = SPELL_ONLY,
			StatModAttribute = SPELL_ONLY,
			StatModVital = SPELL_ONLY,
			StatModSkill = SPELL_ONLY,
			StatModIntProp = SPELL_ONLY,
			StatModFloatProp = SPELL_ONLY,
			DamageType = SPELL_ONLY,
			BaseIntensity = SPELL_ONLY,
			Variance = SPELL_ONLY,
			WeenieClassId = SPELL_ONLY,
			NumProjectiles = SPELL_ONLY,
			NumProjectilesVariance = SPELL_ONLY,
			SpreadAngle = SPELL_ONLY,
			VerticalAngle = SPELL_ONLY,
			DefaultLaunchAngle = SPELL_ONLY,
			NonTracking = SPELL_ONLY,
			CreateOffset = SPELL_ONLY,
			Padding = SPELL_ONLY,
			Dims = SPELL_ONLY,
			Peturbation = SPELL_ONLY,
			ImbuedEffect = SPELL_ONLY,
			SlayerCreatureType = SPELL_ONLY,
			SlayerDamageBonus = SPELL_ONLY,
			CritFrequency = SPELL_ONLY,
			CritMultiplier = SPELL_ONLY,
			IgnoreMagicResist = SPELL_ONLY,
			ElementalModifier = SPELL_ONLY,
			DrainPercentage = SPELL_ONLY,
			DamageRatio = SPELL_ONLY,
			VitalDamageType = SPELL_ONLY,
			Boost = SPELL_ONLY,
			BoostVariance = SPELL_ONLY,
			Source = SPELL_ONLY,
			Destination = SPELL_ONLY,
			Proportion = SPELL_ONLY,
			LossPercent = SPELL_ONLY,
			SourceLoss = SPELL_ONLY,
			TransferCap = SPELL_ONLY,
			MaxBoostAllowed = SPELL_ONLY,
			TransferFlags = SPELL_ONLY,
			Index = SPELL_ONLY,
			Link = SPELL_ONLY,
			DestinationLandcell = SPELL_ONLY,
			DestinationOrigin = SPELL_ONLY,
			DestinationOrientation = SPELL_ONLY,
			MinPower = SPELL_ONLY,
			MaxPower = SPELL_ONLY,
			PowerVariance = SPELL_ONLY,
			DispelSchool = SPELL_ONLY,
			Align = SPELL_ONLY,
			Number = SPELL_ONLY,
			NumberVariance = SPELL_ONLY,
			DotDuration = SPELL_ONLY,
		},
	})

	local SpellBook = class("SpellBook")
	local spellbook_changed
	SpellBook.define({
		get = {
			KnownSpellsIds = function()
				local out = {}
				for id in pairs(known_ids()) do
					table.insert(out, id)
				end
				table.sort(out)
				return list(out)
			end,
			Count = function()
				local n = 0
				for _ in pairs(known_ids()) do
					n = n + 1
				end
				return n
			end,
			OnChanged = function()
				return spellbook_changed
			end,
		},
		call = {
			Get = function(_, id)
				return spell(id)
			end,
			TryGet = function(_, id)
				local sp = spell(id)
				return sp ~= nil, sp
			end,
			IsKnown = function(_, id)
				return known_ids()[num(id)] == true
			end,
			IsValid = function(_, id)
				return D.magic.spell(num(id)) ~= nil
			end,
			HasSkillToCastSpell = function(_, id, modifier)
				local s = D.magic.spell(num(id))
				if s == nil then
					return false
				end
				return magic_skill(s.school or 0) >= (s.difficulty or 0) + (num(modifier) or 20)
			end,
			Dispose = function() end,
		},
		refused = {
			GetSelfBuffs = "choosing the buffs to cast is the plugins' own (the Buffbot's profiles)",
		},
	})

	-- Enchantments ---------------------------------------------------------------------------------

	local Enchantment = class("Enchantment")
	local function enchantment(e)
		return Enchantment.new({ e = e, at = U.now() })
	end
	Enchantment.define({
		get = {
			SpellId = function(x)
				return state(x).e.spell
			end,
			LayeredId = function(x)
				return U.LayeredSpellId.new({ id = state(x).e.spell, layer = 0 })
			end,
			Layer = function()
				return 0
			end,
			Name = function(x)
				return state(x).e.name
			end,
			Category = function(x)
				return state(x).e.category or 0
			end,
			Duration = function(x)
				local r = state(x).e.remaining
				return r ~= nil and r or -1
			end,
			ExpiresAt = function(x)
				local r = state(x).e.remaining
				if r == nil then
					return U.DateTime(253402300799)
				end
				return U.DateTime(state(x).at + r)
			end,
			ClientReceivedAt = function(x)
				return U.DateTime(state(x).at)
			end,
			Beneficial = function(x)
				return state(x).e.beneficial == true
			end,
			HasSpellSetId = function()
				return false
			end,
			Power = function(x)
				local s = D.magic.spell(state(x).e.spell)
				return s and s.difficulty or 0
			end,
		},
		refused = {
			StatKey = "the plugin interface does not read an enchantment's effect",
			StatValue = "the plugin interface does not read an enchantment's effect",
			Flags = "the plugin interface does not read an enchantment's effect",
			CasterId = "the plugin interface does not tell who cast an enchantment",
		},
	})
	Enchantment.tostring = function(x)
		return tostring(state(x).e.name)
	end
	local EFFECT_ONLY = "the plugin interface reads an enchantment's spell, name, category and time left"
	Enchantment.define({
		refused = {
			StartTime = EFFECT_ONLY,
			DegradeModifier = EFFECT_ONLY,
			DegradeLimit = EFFECT_ONLY,
			LastTimeDegraded = EFFECT_ONLY,
			SpellSetId = EFFECT_ONLY,
			Effect = EFFECT_ONLY,
		},
	})
	local function enchantments()
		local out = {}
		for _, e in ipairs(D.game.player.enchantments or {}) do
			table.insert(out, enchantment(e))
		end
		return list(out)
	end

	-- Fellowship -----------------------------------------------------------------------------------

	local FellowshipMember = class("FellowshipMember")
	FellowshipMember.define({
		get = {
			Id = function(m)
				return state(m).m.id
			end,
			Name = function(m)
				return state(m).m.name
			end,
			Level = function(m)
				return state(m).m.level or 0
			end,
			MaxHealth = function(m)
				return (state(m).m.health or {}).max or 0
			end,
			MaxStamina = function(m)
				return (state(m).m.stamina or {}).max or 0
			end,
			MaxMana = function(m)
				return (state(m).m.mana or {}).max or 0
			end,
			CurrentHealth = function(m)
				return (state(m).m.health or {}).current or 0
			end,
			CurrentStamina = function(m)
				return (state(m).m.stamina or {}).current or 0
			end,
			CurrentMana = function(m)
				return (state(m).m.mana or {}).current or 0
			end,
		},
	})
	FellowshipMember.tostring = function(m)
		return tostring(state(m).m.name)
	end

	local Fellowship = class("Fellowship")
	local fellow_events = {}
	local function fs()
		return D.game.player.fellowship
	end
	Fellowship.define({
		get = {
			Exists = function()
				return fs() ~= nil
			end,
			IsOpen = function()
				local f = fs()
				return f ~= nil and f.open == true
			end,
			Name = function()
				local f = fs()
				return f and f.name or ""
			end,
			LeaderId = function()
				local f = fs()
				return f and f.leader or 0
			end,
			ShareExp = function()
				local f = fs()
				return f ~= nil and f.share_xp == true
			end,
			EvenExpSharing = function()
				local f = fs()
				return f ~= nil and f.even_split == true
			end,
			Locked = function()
				local f = fs()
				return f ~= nil and f.locked == true
			end,
			Members = function()
				local out = {}
				local f = fs()
				for _, m in ipairs(f and f.members or {}) do
					out[m.id] = FellowshipMember.new({ m = m })
				end
				return dict(out)
			end,
			MAX_FELLOW_COUNT = function()
				return 9
			end,
			OnJoin = function()
				return fellow_events.join
			end,
			OnLeave = function()
				return fellow_events.leave
			end,
			OnMembersChanged = function()
				return fellow_events.members
			end,
		},
		call = {
			Create = function(_, name, share, options, cb)
				return U.Actions.FellowCreate(name, share, options, cb)
			end,
			Disband = function(_, options, cb)
				return U.Actions.FellowDisband(options, cb)
			end,
			Dismiss = function(_, id, options, cb)
				return U.Actions.FellowDismiss(id, options, cb)
			end,
			SetLeader = function(_, id, options, cb)
				return U.Actions.FellowSetLeader(id, options, cb)
			end,
			Quit = function(_, disband, options, cb)
				return U.Actions.FellowQuit(disband, options, cb)
			end,
			Recruit = function(_, id, options, cb)
				return U.Actions.FellowRecruit(id, options, cb)
			end,
			SetOpen = function(_, open, options, cb)
				return U.Actions.FellowSetOpen(open, options, cb)
			end,
			Dispose = function() end,
		},
	})

	-- Allegiance -----------------------------------------------------------------------------------

	local AllegianceMember = class("AllegianceMember")
	AllegianceMember.define({
		get = {
			Id = function(m)
				return state(m).m.id or 0
			end,
			Name = function(m)
				return state(m).m.name or ""
			end,
			Rank = function(m)
				return state(m).m.rank or 0
			end,
			Online = function(m)
				return state(m).m.online == true
			end,
			Experience = function(m)
				return state(m).m.passed_up or 0
			end,
			ParentId = function()
				return 0
			end,
			Level = function()
				return 0
			end,
		},
	})
	local MEMBER_ONLY = "the plugin interface reads an allegiance member's name, rank and whether they are online"
	AllegianceMember.define({
		refused = {
			Gender = MEMBER_ONLY,
			Heritage = MEMBER_ONLY,
			Loyalty = MEMBER_ONLY,
			Leadership = MEMBER_ONLY,
			Flags = MEMBER_ONLY,
		},
	})
	local function member(m)
		if m == nil then
			return nil
		end
		return AllegianceMember.new({ m = m })
	end
	local Allegiance = class("Allegiance")
	local function al()
		return D.game.player.allegiance
	end
	Allegiance.define({
		get = {
			Exists = function()
				return al() ~= nil
			end,
			Name = function()
				local a = al()
				return a and a.name or ""
			end,
			TotalMembers = function()
				local a = al()
				return a and a.members or 0
			end,
			TotalVassals = function()
				local a = al()
				return a and a.vassals and #a.vassals or 0
			end,
			Rank = function()
				local a = al()
				return a and a.rank or 0
			end,
			Monarch = function()
				local a = al()
				return member(a and a.monarch)
			end,
			Patron = function()
				local a = al()
				return member(a and a.patron)
			end,
			Vassals = function()
				local a = al()
				local out = {}
				for _, v in ipairs(a and a.vassals or {}) do
					table.insert(out, member(v))
				end
				return list(out)
			end,
		},
		call = {
			SwearTo = function(_, id, options, cb)
				return U.Actions.AllegianceSwear(id, options, cb)
			end,
			Break = function(_, options, cb)
				return U.Actions.AllegianceBreak(nil, options, cb)
			end,
			Dispose = function() end,
		},
		refused = {
			MOTD = "the plugin interface does not read the allegiance's message of the day",
			BindPoint = "the plugin interface does not read the allegiance's bind point",
			ChatRoomId = "the plugin interface does not read the allegiance's chat room",
			Locked = "the plugin interface does not read whether the allegiance is locked",
		},
	})

	-- Trade ----------------------------------------------------------------------------------------

	local Trade = class("Trade")
	local trade_events = {}
	local function ti()
		return D.trade.info()
	end
	Trade.define({
		get = {
			IsOpen = function()
				return ti() ~= nil
			end,
			PartnerId = function()
				local t = ti()
				return t and t.partner or 0
			end,
			YourItemIds = function()
				local t = ti()
				return list(t and table.clone(t.mine or {}) or {})
			end,
			PartnerItemIds = function()
				local t = ti()
				return list(t and table.clone(t.theirs or {}) or {})
			end,
			YouAccepted = function()
				local t = ti()
				return t ~= nil and t.accepted == true
			end,
			PartnerAccepted = function()
				local t = ti()
				return t ~= nil and t.partner_accepted == true
			end,
		},
		call = {
			Dispose = function() end,
		},
	})
	for _, n in ipairs({ "OnReset", "OnStarted", "OnDeclined", "OnObjectAdded", "OnEnded", "OnCompleted", "OnAccepted", "OnFailedToAddObject", "OnFailed" }) do
		Trade.get[norm(n)] = function()
			return trade_events[n]
		end
	end

	-- Vendor ---------------------------------------------------------------------------------------

	local ItemProfile = class("ItemProfile")
	ItemProfile.define({
		get = {
			ObjectId = function(p)
				return state(p).r.id
			end,
			Id = function(p)
				return state(p).r.id
			end,
			Name = function(p)
				return state(p).r.name
			end,
			Amount = function(p)
				return state(p).r.amount or -1
			end,
			Price = function(p)
				return state(p).r.price
			end,
			Item = function(p)
				return wo(state(p).r.id)
			end,
		},
	})
	local Vendor = class("Vendor")
	local vendor_events = {}
	local function vi()
		return D.vendor.info()
	end
	Vendor.define({
		get = {
			IsOpen = function()
				return vi() ~= nil
			end,
			VendorId = function()
				local v = vi()
				return v and v.id or 0
			end,
			Items = function()
				local v = vi()
				local out = {}
				for _, r in ipairs(v and v.stock or {}) do
					table.insert(out, ItemProfile.new({ r = r }))
				end
				return list(out)
			end,
			OnOpened = function()
				return vendor_events.opened
			end,
			OnClosed = function()
				return vendor_events.closed
			end,
		},
		call = {
			Dispose = function() end,
		},
		refused = {
			MinBuyValue = "the plugin interface reads a vendor's wares and prices, not its terms",
			MaxBuyValue = "the plugin interface reads a vendor's wares and prices, not its terms",
			Magic = "the plugin interface reads a vendor's wares and prices, not its terms",
			BuyPriceModifier = "the plugin interface reads a vendor's wares and prices, not its terms",
			SellPriceModifier = "the plugin interface reads a vendor's wares and prices, not its terms",
			CurrencyType = "the plugin interface reads a vendor's wares and prices, not its terms",
			CurrencyName = "the plugin interface reads a vendor's wares and prices, not its terms",
			PlayerCurrencyAmount = "the plugin interface reads a vendor's wares and prices, not its terms",
			Category = "the plugin interface reads a vendor's wares and prices, not its terms",
			AddToBuyList = "add an item with game.Actions.VendorAddToBuyList",
		},
	})

	-- Where the character is -----------------------------------------------------------------------

	local PhysicsEnvironment = class("PhysicsEnvironment")
	local function here()
		return D.game.player.position
	end
	PhysicsEnvironment.define({
		get = {
			CurrentLandcell = function()
				local p = here()
				return p and p.cell or 0
			end,
			CurrentLandblock = function()
				local p = here()
				return p and (p.cell - p.cell % 65536) or 0
			end,
			IsOutside = function()
				return D.game.player.outside == true
			end,
			IsInside = function()
				return D.game.player.outside ~= true
			end,
			IsDungeon = function()
				local p = here()
				if p == nil or D.game.player.outside == true then
					return false
				end
				-- A building's rooms sit in a landblock with land; a dungeon's in one of cells alone.
				local lb = D.game.data and D.game.data.landblock and D.game.data.landblock(p.cell)
				return lb == nil or lb.heights == nil
			end,
			CanSeeOutside = function()
				return D.game.player.outside == true
			end,
		},
		call = {
			IsLandcellVisible = function(_, cell)
				local p = here()
				return p ~= nil and (p.cell - p.cell % 65536) == (num(cell) - num(cell) % 65536)
			end,
		},
		refused = {
			VisibleLandblocks = "the plugin interface does not list what the client draws",
			VisibleLandcells = "the plugin interface does not list what the client draws",
		},
	})

	-- The world ------------------------------------------------------------------------------------

	local World = class("World")

	-- What `GetAll` and its kin take: an object class (typed), an object type (a number), a name, or a
	-- function.
	local function matcher(arg)
		if arg == nil then
			return function()
				return true
			end
		end
		if type(arg) == "function" then
			return function(o)
				return arg(o) == true
			end
		end
		if type(arg) == "string" then
			return function(o)
				return state(o).h.name == arg
			end
		end
		if U.enum_of(arg) == "ObjectClass" then
			local c = num(arg)
			return function(o)
				return (state(o).h.class_id or 0) == c
			end
		end
		local t = num(arg)
		return function(o)
			return (state(o).h.type or 0) == t
		end
	end

	-- Every object the client knows, as world objects, in id order.
	local function all()
		return U.objects(D.game.world:find({}))
	end

	local function nearest(arg, dtype)
		local m = matcher(arg)
		local flat = U.enum_name("DistanceType", num(dtype) or 1) == "T2D"
		local me = D.game.player.id
		local here_at = U.place_of(me)
		local best, bestd = nil, math.huge
		for _, o in ipairs(all()) do
			if state(o).id ~= me and m(o) then
				local d = U.distance(here_at, U.place_of(state(o).id), flat)
				if d < bestd then
					best, bestd = o, d
				end
			end
		end
		return best
	end

	local function landscape()
		local out = {}
		for _, o in ipairs(all()) do
			local h = state(o).h
			if h.container == nil and h.wielder == nil then
				table.insert(out, o)
			end
		end
		return out
	end

	local function filtered(objs, arg)
		local m = matcher(arg)
		local out = {}
		for _, o in ipairs(objs) do
			if m(o) then
				table.insert(out, o)
			end
		end
		return list(out)
	end

	local world_events = {}
	local chat_events = {}

	World.define({
		get = {
			OpenContainer = function()
				return wo(D.game.inventory.open_container)
			end,
			PhysicsEnvironment = function()
				return PhysicsEnvironment.new({})
			end,
			Vendor = function()
				return Vendor.new({})
			end,
			Selected = function()
				local s = D.game.selection()
				return s and wo(s.id) or nil
			end,
		},
		call = {
			Get = function(_, id)
				return wo(num(id))
			end,
			TryGet = function(_, id)
				local o = wo(num(id))
				return o ~= nil, o
			end,
			Exists = function(_, id)
				return D.game.world:get(num(id)) ~= nil
			end,
			GetAll = function(_, arg)
				return filtered(all(), arg)
			end,
			GetNearest = function(_, arg, dtype)
				return nearest(arg, dtype)
			end,
			GetLandscape = function(_, arg)
				return filtered(landscape(), arg)
			end,
			Dispose = function() end,
		},
	})
	World.index = function(_, id)
		if type(id) == "number" then
			return wo(id)
		end
		return nil
	end
	for _, n in ipairs({ "OnObjectSelected", "OnContainerOpened", "OnContainerClosed", "OnTick", "OnObjectCreated", "OnObjectReleased", "OnConfirmationRequest" }) do
		World.get[norm(n)] = function()
			return world_events[n]
		end
	end
	for _, n in ipairs({ "OnChannelMessage", "OnChatText", "OnTell", "OnLocalMessage", "OnEmote", "OnSoulEmote", "OnFellowMessage" }) do
		World.get[norm(n)] = function()
			return chat_events[n]
		end
	end
	World.get[norm("OnChatInput")] = function()
		return world_events.OnChatInput
	end
	World.get[norm("OnChatNameClicked")] = function()
		return world_events.OnChatNameClicked
	end

	-- Chat ---------------------------------------------------------------------------------------

	-- What the server said each line, by message, kept until a chat line of the frame takes it: the
	-- speaker and the words of a speech, a tell, an emote, a fellowship line.
	local said = {}
	local SPEECH = {
		Communication_HearSpeech = { "message", "sender_name", "sender_id" },
		Communication_HearRangedSpeech = { "message", "sender_name", "sender_id" },
		Communication_HearDirectSpeech = { "message", "sender_name", "sender_id" },
		Communication_HearEmote = { "text", "sender_name", "sender" },
		Communication_HearSoulEmote = { "text", "sender_name", "sender" },
		Communication_ChannelBroadcast = { "message", "sender_name", nil },
	}
	local KIND_EVENT = {
		speech = "OnLocalMessage",
		tell = "OnTell",
		emote = "OnEmote",
		social = "OnSoulEmote",
		fellowship = "OnFellowMessage",
		channel = "OnChannelMessage",
		all_channels = "OnChannelMessage",
	}
	local ROOMS = {
		allegiance = 1,
		general = 2,
		trade = 3,
		lfg = 4,
		roleplay = 5,
		olthoi = 6,
		society = 7,
		["celestial hand"] = 8,
		["eldrytch web"] = 9,
		["radiant blood"] = 10,
	}

	-- A line's speaker and words when no message told them: the forms the game writes them in.
	local function parse_line(kind, text)
		-- A name the game marks up as a link to the one it names is that name.
		text = string.gsub(text, "<Tell:[^>]*>(.-)<\\Tell>", "%1")
		local name, words = string.match(text, '^(.-) tells you, "(.*)"$')
		if name ~= nil then
			return name, words, 0
		end
		local room
		room, name, words = string.match(text, '^%[(.-)%] (.-) says, "(.*)"$')
		if room ~= nil then
			return name, words, ROOMS[string.lower(room)] or 0
		end
		name, words = string.match(text, '^(.-) says, "(.*)"$')
		if name ~= nil then
			return name, words, 0
		end
		return "", text, 0
	end

	local function start_chat()
		if D.host.has("messages") then
			for name, f in pairs(SPEECH) do
				listen("messages." .. name, function(m)
					if m.dir == "received" and m.fields ~= nil then
						table.insert(said, {
							message = m.fields[f[1]] or "",
							name = m.fields[f[2]] or "",
							id = f[3] and m.fields[f[3]] or 0,
						})
					end
				end)
			end
		end
		listen("chat.line", function(e)
			local text = e.text or ""
			local kind = e.kind
			local fact
			for i, s in ipairs(said) do
				if s.message ~= "" and string.find(text, s.message, 1, true) then
					fact = s
					table.remove(said, i)
					break
				end
			end
			local name, words, room = parse_line(kind, text)
			if fact ~= nil then
				name, words = fact.name, fact.message
			end
			local eaten = false
			local a = args("ChatEventArgs", {
				SenderId = fact and fact.id or 0,
				SenderName = name,
				Message = words,
				Room = ev("ChatChannel", room),
				Type = ev("ChatMessageType", e.type or 0),
				Eat = false,
			})
			state(a).on_eat = function(v)
				if v and not eaten then
					eaten = true
					pcall(D.chat.hide, e)
				end
			end
			local specific = KIND_EVENT[kind]
			if specific ~= nil then
				fire(chat_events[specific], a)
			end
			fire(chat_events.OnChatText, a)
		end)
		-- What no line took this frame is not this frame's.
		U.every(1000, function()
			said = {}
		end)
	end

	-- The character ----------------------------------------------------------------------------------

	local Character = class("Character")
	local char_events = {}
	local function inventory()
		local out = {}
		for _, id in ipairs(U.all_item_ids(D.game.player.id)) do
			local o = wo(id)
			if o ~= nil then
				table.insert(out, o)
			end
		end
		return out
	end
	local function stack_of(o)
		local h = state(o).h
		return h.stack or 1
	end
	local VITAL_FIELD = { [1] = "health", [3] = "stamina", [5] = "mana" }
	local function stat_delta(list_name, id)
		for _, s in ipairs(D.game.player[list_name] or {}) do
			if s.id == id then
				return (s.current or 0) - (s.base or 0), s
			end
		end
		return 0, nil
	end
	Character.define({
		get = {
			Id = function()
				return D.game.player.id or 0
			end,
			Weenie = function()
				return wo(D.game.player.id)
			end,
			Inventory = function()
				return list(inventory())
			end,
			Containers = function()
				local _, packs = U.contents(D.game.player.id)
				return U.list_of_ids(packs)
			end,
			Equipment = function()
				local out = {}
				for _, h in ipairs(D.game.world:find({ wielder = D.game.player.id })) do
					table.insert(out, h.id)
				end
				return U.list_of_ids(out)
			end,
			Vitae = function()
				return D.game.player.vitae or 1
			end,
			Fellowship = function()
				return Fellowship.new({})
			end,
			Allegiance = function()
				return Allegiance.new({})
			end,
			Trade = function()
				return Trade.new({})
			end,
			SpellBook = function()
				return SpellBook.new({})
			end,
			InPortalSpace = function()
				return D.session.info().portal_space == true
			end,
			Stance = function()
				return wo(D.game.player.id).Stance
			end,
			CombatMode = function()
				return ev("CombatMode", D.game.player.combat_mode or 1)
			end,
			BurdenUnits = function()
				local i = D.game.player.info
				return i and i.encumbrance or 0
			end,
			MaxBurdenUnits = function()
				local i = D.game.player.info
				return i and i.capacity or 0
			end,
			BusyTracker = function()
				return U.BusyTracker
			end,
			Options1 = function()
				return 0
			end,
		},
		call = {
			SharedCooldowns = function()
				return list({})
			end,
			AllEnchantments = function()
				return enchantments()
			end,
			ActiveEnchantments = function()
				return enchantments()
			end,
			GetMagicSkill = function(_, school)
				return magic_skill(school)
			end,
			-- What the character's enchantments add to one of its skills, attributes or vitals:
			-- the plugin interface tells its current and base values, and they differ by it.
			GetEnchantmentsAdditiveModifier = function(_, id)
				local e = U.enum_of(id)
				if e == "SkillId" then
					return (stat_delta("skills", num(id)))
				elseif e == "AttributeId" then
					return (stat_delta("attributes", num(id)))
				elseif e == "VitalId" then
					return (stat_delta("vital_stats", num(id)))
				end
				error("GetEnchantmentsAdditiveModifier needs a SkillId, AttributeId or VitalId", 2)
			end,
			GetInventoryCount = function(_, arg)
				local n = 0
				for _, o in ipairs(filtered(inventory(), arg)) do
					n = n + stack_of(o)
				end
				return n
			end,
			GetFirstInventory = function(_, arg)
				return filtered(inventory(), arg)[1]
			end,
			GetInventory = function(_, arg)
				return filtered(inventory(), arg)
			end,
			Dispose = function() end,
		},
		refused = {
			GetActiveEnchantments = "the plugin interface does not read which statistic an enchantment changes",
			GetEnchantmentsMultiplierModifier = "the plugin interface does not read an enchantment's effect",
			AnimTracker = "the plugin interface does not follow the character's animations",
		},
	})
	for _, n in ipairs({ "OnPortalSpaceEntered", "OnPortalSpaceExited", "OnSharedCooldownsChanged", "OnEnchantmentsChanged", "OnVitalChanged", "OnTotalExperienceChanged", "OnAvailableLuminanceChanged", "OnLevelChanged", "OnVitaeChanged", "OnCastSpellAttempt", "OnDeath" }) do
		Character.get[norm(n)] = function()
			return char_events[n]
		end
	end

	-- The busy tracker: whether the character is busy with an item or a cast.
	local BusyTracker = class("BusyTracker")
	BusyTracker.define({
		get = {
			CurrentAction = function()
				return nil
			end,
			Actions = function()
				return list({})
			end,
		},
		call = {
			IsBusy = function()
				return D.game.player.busy == true
			end,
			Dispose = function() end,
		},
	})
	U.BusyTracker = BusyTracker.new({})

	-- The game -------------------------------------------------------------------------------------

	local Game = class("Game")
	local CharacterIdentity = class("CharacterIdentity")
	CharacterIdentity.define({
		get = {
			Id = function(c)
				return state(c).c.id
			end,
			Name = function(c)
				return state(c).c.name
			end,
			SecondsGreyedOut = function()
				return 0
			end,
		},
	})
	CharacterIdentity.tostring = function(c)
		return tostring(state(c).c.name)
	end

	local Capabilities = class("Capabilities")
	Capabilities.define({
		call = {
			Has = function(_, cap)
				return num(cap) == 1
			end,
			Get = function()
				return 1
			end,
		},
	})

	Game.define({
		get = {
			State = function()
				return ev("ClientState", U.client_state())
			end,
			CharacterId = function()
				return U.in_world() and (D.game.player.id or 0) or 0
			end,
			Character = function(g)
				if not U.in_world() then
					return nil
				end
				return state(g).character
			end,
			World = function(g)
				if not U.in_world() then
					return nil
				end
				return state(g).world
			end,
			AccountName = function()
				return D.session.info().account or ""
			end,
			ServerName = function()
				return D.session.info().world or ""
			end,
			Characters = function()
				local out = {}
				for _, c in ipairs(D.session.info().characters or {}) do
					table.insert(out, CharacterIdentity.new({ c = c }))
				end
				return list(out)
			end,
			MaxAllowedCharacters = function()
				return #(D.session.info().characters or {})
			end,
			LastTick = function(g)
				return U.DateTime(state(g).last_tick or 0)
			end,
			Capabilities = function()
				return Capabilities.new({})
			end,
			Actions = function()
				return U.client_actions
			end,
			ActionQueue = function()
				return U.action_queue
			end,
			Messages = function()
				return U.message_handler()
			end,
		},
	})
	for _, n in ipairs({ "OnStateChanged", "OnTick", "OnRender2D", "OnRender3D", "OnScriptEnd", "OnWindowMessage" }) do
		Game.get[norm(n)] = function(g)
			return state(g).events[n]
		end
	end
	function U.game_event(g, name)
		return state(g).events[name]
	end

	-- Hearing the game: each event of the script's begins listening with its first handler.
	local function make_events(g)
		local gs = state(g)
		local e = {}
		gs.events = e
		e.OnScriptEnd = event("OnScriptEnd")
		e.OnTick = event("OnTick", function()
			U.every(1000, function()
				gs.last_tick = U.now()
				fire(e.OnTick, nil)
			end)
		end)
		local function render(name)
			e[name] = event(name, function()
				U.every(50, function()
					fire(e[name], nil)
				end)
			end)
		end
		render("OnRender2D")
		render("OnRender3D")
		e.OnWindowMessage = event("OnWindowMessage", nil, "a plugin never sees the keyboard or the mouse; declare a hotkey for a command")
		e.OnStateChanged = event("OnStateChanged", function()
			local last = U.client_state()
			local function to(st)
				return function()
					if st == last then
						return
					end
					last = st
					fire(e.OnStateChanged, args("StateChangedEventArgs", { NewState = ev("ClientState", st) }))
				end
			end
			listen("session.pregame", to(2))
			local entering = to(4)
			listen("session.login", function()
				entering()
				-- A client already in the world by now is in the game too.
				if U.client_state() == 6 then
					to(6)()
				end
			end)
			listen("session.enter_world", to(6))
			listen("session.logout", to(7))
			listen("session.disconnect", to(8))
			-- The character in the world: in the game, whichever message said so.
			local in_game = to(6)
			listen("context.begin", function(m)
				if m.context == "world" then
					in_game()
				end
			end)
			listen("session.portal_exit", function()
				if U.client_state() == 6 then
					in_game()
				end
			end)
		end)

		-- The world's events.
		world_events.OnTick = event("OnTick", function()
			U.every(1000, function()
				fire(world_events.OnTick, nil)
			end)
		end)
		world_events.OnObjectCreated = event("OnObjectCreated", function()
			listen("world.object_created", function(m)
				fire(world_events.OnObjectCreated, args("ObjectCreatedEventArgs", { ObjectId = m.id }))
			end)
		end)
		world_events.OnObjectReleased = event("OnObjectReleased", function()
			listen("world.object_removed", function(m)
				fire(world_events.OnObjectReleased, args("ObjectReleasedEventArgs", { ObjectId = m.id }))
			end)
		end)
		world_events.OnObjectSelected = event("OnObjectSelected", function()
			listen("world.selection_changed", function(m)
				fire(world_events.OnObjectSelected, args("ObjectSelectedEventArgs", { ObjectId = m.id or 0 }))
			end)
		end)
		world_events.OnContainerOpened = event("OnContainerOpened", function()
			listen("inventory.container_opened", function(m)
				fire(world_events.OnContainerOpened, args("ContainerOpenedEventArgs", { Container = wo(m.id) }))
			end)
		end)
		world_events.OnContainerClosed = event("OnContainerClosed", function()
			listen("inventory.container_closed", function(m)
				fire(world_events.OnContainerClosed, args("ContainerClosedEventArgs", { Container = wo(m.id) }))
			end)
		end)
		world_events.OnConfirmationRequest = event("OnConfirmationRequest", function()
			if not D.host.has("messages") then
				return
			end
			listen("messages.Character_ConfirmationRequest", function(m)
				local f = m.fields or {}
				fire(
					world_events.OnConfirmationRequest,
					args("ConfirmationRequestEventArgs", {
						Type = ev("ClientConfirmationType", f.confirmation_type or 0),
						Text = f.text or "",
						ClickYes = false,
						ClickNo = false,
					})
				)
			end)
		end)
		world_events.OnChatInput = event(
			"OnChatInput",
			nil,
			"a plugin hears only the commands it claims; claim one with commands.add"
		)
		world_events.OnChatNameClicked = event("OnChatNameClicked", nil, "a plugin does not hear clicks in the chat")
		local chat_started = false
		local function start_chat_once()
			if not chat_started then
				chat_started = true
				start_chat()
			end
		end
		for _, n in ipairs({ "OnChannelMessage", "OnChatText", "OnTell", "OnLocalMessage", "OnEmote", "OnSoulEmote", "OnFellowMessage" }) do
			chat_events[n] = event(n, start_chat_once)
		end

		-- The character's events.
		local c = char_events
		c.OnPortalSpaceEntered = event("OnPortalSpaceEntered", function()
			listen("session.portal_enter", function()
				fire(c.OnPortalSpaceEntered, nil)
			end)
		end)
		c.OnPortalSpaceExited = event("OnPortalSpaceExited", function()
			listen("session.portal_exit", function()
				fire(c.OnPortalSpaceExited, nil)
			end)
		end)
		c.OnSharedCooldownsChanged = event("OnSharedCooldownsChanged")
		c.OnEnchantmentsChanged = event("OnEnchantmentsChanged", function()
			local function heard(kind)
				return function(m)
					fire(
						c.OnEnchantmentsChanged,
						args("EnchantmentsChangedEventArgs", {
							Type = ev("AddRemoveEventType", kind),
							LayeredSpellId = U.LayeredSpellId.new({ id = m.spell, layer = 0 }),
							Enchantment = enchantment({ spell = m.spell, name = m.name, remaining = m.remaining }),
						})
					)
				end
			end
			listen("character.enchantment_added", heard(0))
			listen("character.enchantment_removed", heard(1))
		end)
		c.OnVitalChanged = event("OnVitalChanged", function()
			local ids = { health = 1, stamina = 3, mana = 5 }
			listen("character.vital", function(m)
				fire(c.OnVitalChanged, args("VitalChangedEventArgs", {
					Type = ev("VitalId", ids[m.vital] or 1),
					Value = m.current,
					OldValue = m.previous or m.current,
				}))
			end)
		end)
		c.OnTotalExperienceChanged = event("OnTotalExperienceChanged", function()
			listen("character.xp", function(m)
				fire(c.OnTotalExperienceChanged, args("TotalExperienceChangedEventArgs", {
					TotalExperience = m.total,
					OldTotalExperience = (m.total or 0) - (m.gained or 0),
				}))
			end)
		end)
		c.OnAvailableLuminanceChanged = event("OnAvailableLuminanceChanged", function()
			listen("character.luminance", function(m)
				fire(c.OnAvailableLuminanceChanged, args("AvailableLuminanceChangedEventArgs", {
					AvailableLuminance = m.earned,
					OldAvailableLuminance = (m.earned or 0) - (m.gained or 0),
				}))
			end)
		end)
		-- The level and the vitae, which change without an event of their own, are looked at as the
		-- character's experience and vitals change, and every second.
		local last_level, last_vitae
		local function watch()
			local p = D.game.player
			local level, vitae = p.level, p.vitae
			if last_level ~= nil and level ~= nil and level ~= last_level then
				fire(c.OnLevelChanged, args("LevelChangedEventArgs", { Level = level, OldLevel = last_level }))
			end
			if last_vitae ~= nil and vitae ~= nil and vitae ~= last_vitae then
				fire(c.OnVitaeChanged, args("VitaeChangedEventArgs", { Vitae = vitae, OldVitae = last_vitae }))
			end
			last_level, last_vitae = level, vitae
		end
		local watching = false
		local function start_watch()
			if watching then
				return
			end
			watching = true
			last_level, last_vitae = D.game.player.level, D.game.player.vitae
			listen("character.xp", watch)
			listen("character.vital", watch)
			listen("character.death", watch)
			U.every(1000, watch)
		end
		c.OnLevelChanged = event("OnLevelChanged", start_watch)
		c.OnVitaeChanged = event("OnVitaeChanged", start_watch)
		c.OnCastSpellAttempt = event("OnCastSpellAttempt", function()
			if not D.host.has("messages") then
				return
			end
			local function heard(targeted)
				return function(m)
					if m.dir ~= "sent" then
						return
					end
					local f = m.fields or {}
					fire(c.OnCastSpellAttempt, args("CastSpellAttemptEventArgs", {
						SpellId = f.spell_id or 0,
						TargetId = f.target or 0,
						EventType = ev("CastEventType", targeted and 1 or 0),
					}))
				end
			end
			listen("messages.Magic_CastTargetedSpell", heard(true))
			listen("messages.Magic_CastUntargetedSpell", heard(false))
		end)
		c.OnDeath = event("OnDeath", function()
			listen("character.death", function()
				fire(c.OnDeath, args("DeathEventArgs", { Text = "", KillerId = 0 }))
			end)
		end)

		-- The fellowship's.
		fellow_events.join = event("OnJoin", function()
			listen("character.fellowship", function(m)
				if m.change == "joined" then
					fire(fellow_events.join, nil)
				end
			end)
		end)
		fellow_events.leave = event("OnLeave", function()
			listen("character.fellowship", function(m)
				if m.change == "left" then
					fire(fellow_events.leave, nil)
				end
			end)
		end)
		fellow_events.members = event("OnMembersChanged", function()
			local before = {}
			for _, m in ipairs((fs() or {}).members or {}) do
				before[m.id] = m
			end
			listen("character.fellowship", function(m)
				local now = {}
				for _, x in ipairs((fs() or {}).members or {}) do
					now[x.id] = x
				end
				for id, x in pairs(now) do
					if before[id] == nil then
						fire(fellow_events.members, args("FellowshipChangedEventArgs", {
							Type = ev("AddRemoveEventType", 0),
							ObjectId = id,
							Member = FellowshipMember.new({ m = x }),
						}))
					end
				end
				for id, x in pairs(before) do
					if now[id] == nil then
						fire(fellow_events.members, args("FellowshipChangedEventArgs", {
							Type = ev("AddRemoveEventType", 1),
							ObjectId = id,
							Member = FellowshipMember.new({ m = x }),
						}))
					end
				end
				before = now
			end)
		end)

		-- The trade's: what changed, from the trade as it was before.
		local last_trade
		local trade_listening = false
		local function start_trade()
			if trade_listening then
				return
			end
			trade_listening = true
			last_trade = ti()
			listen("commerce.trade_opened", function(m)
				last_trade = ti()
				fire(trade_events.OnStarted, args("TradeStartedEventArgs", { InitiatorId = 0, PartnerId = m.partner or 0 }))
			end)
			listen("commerce.trade_changed", function()
				local t = ti()
				if t == nil then
					return
				end
				local was = last_trade or { mine = {}, theirs = {} }
				local function added(now_ids, was_ids, side)
					local seen = {}
					for _, id in ipairs(was_ids or {}) do
						seen[id] = true
					end
					for _, id in ipairs(now_ids or {}) do
						if not seen[id] then
							fire(trade_events.OnObjectAdded, args("ObjectAddedToTradeEventArgs", {
								ObjectId = id,
								Side = ev("TradeSide", side),
							}))
						end
					end
				end
				if #(t.mine or {}) == 0 and #(t.theirs or {}) == 0 and (#(was.mine or {}) > 0 or #(was.theirs or {}) > 0) then
					fire(trade_events.OnReset, args("TradeResetEventArgs", { ResetterObjectId = 0 }))
				end
				added(t.mine, was.mine, 1)
				added(t.theirs, was.theirs, 2)
				if t.accepted and not was.accepted then
					fire(trade_events.OnAccepted, args("TradeAcceptedEventArgs", { AcceptorId = D.game.player.id }))
				end
				if t.partner_accepted and not was.partner_accepted then
					fire(trade_events.OnAccepted, args("TradeAcceptedEventArgs", { AcceptorId = t.partner }))
				end
				if was.partner_accepted and not t.partner_accepted and #(t.theirs or {}) > 0 then
					fire(trade_events.OnDeclined, args("TradeDeclinedEventArgs", { DeclinerId = t.partner }))
				end
				last_trade = t
			end)
			listen("commerce.trade_closed", function()
				local was = last_trade
				if was ~= nil and was.accepted and was.partner_accepted then
					fire(trade_events.OnCompleted, args("TradeCompletedEventArgs", {
						PartnerId = was.partner,
						YourItemIds = list(table.clone(was.mine or {})),
						PartnerItemIds = list(table.clone(was.theirs or {})),
					}))
				end
				fire(trade_events.OnEnded, args("TradeEndedEventArgs", { Reason = ev("EndTradeReason", 0) }))
				last_trade = nil
			end)
		end
		for _, n in ipairs({ "OnReset", "OnStarted", "OnDeclined", "OnObjectAdded", "OnEnded", "OnCompleted", "OnAccepted", "OnFailedToAddObject", "OnFailed" }) do
			trade_events[n] = event(n, start_trade)
		end

		-- The vendor's.
		vendor_events.opened = event("OnOpened", function()
			listen("commerce.vendor_opened", function()
				fire(vendor_events.opened, nil)
			end)
		end)
		vendor_events.closed = event("OnClosed", function()
			listen("commerce.vendor_closed", function()
				fire(vendor_events.closed, nil)
			end)
		end)

		spellbook_changed = event("OnChanged")
	end

	-- A new `game` for a script's run: its world and character, and its events, none listening yet.
	function U.new_game(r)
		local g = Game.new({
			world = World.new({}),
			character = Character.new({}),
		})
		make_events(g)
		-- When the client first knew each object, and when each appraisal arrived, for the objects'
		-- `CreatedAt` and `LastAppraisalResponse`.
		listen("world.object_created", function(m)
			created[m.id] = U.now()
		end)
		listen("items.appraised", function(m)
			appraised[m.id] = U.now()
		end)
		return g
	end
end
