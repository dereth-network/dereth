-- The macro's settings. They carry VTank's option names and units, so a VTank settings profile
-- and a meta's "set option" mean the same here: distances in map units (240 metres; VTank's
-- "landblocks"), vital marks in percent. Names are matched without case, as VTank matches them.
-- A few settings are this plugin's own and say so.

local S = {}

-- One map unit in metres.
S.UNIT = 240

-- name -> default. VTank's names first, then this plugin's own.
S.defaults = {
	-- The macro's parts.
	EnableCombat = true,
	EnableNav = true,
	EnableLooting = true,
	EnableBuffing = true,
	EnableMeta = true,

	-- Combat. AttackDistance: monsters nearer than this are engaged. ApproachDistance: with
	-- navigation on, a monster this near is run toward until it is in attack distance.
	AttackDistance = 8 / 240,
	AttackMinimumDistance = 0,
	ApproachDistance = 20 / 240,
	DefaultMeleeAttackHeight = 2, -- 1 high, 2 mid, 3 low
	-- The monster the player selects is attacked first while it is a target the macro would fight,
	-- unless a monster of higher priority is about.
	TargetLock = false,
	-- Which of the monsters of one priority is fought first: 1 the nearest, 2 the one the
	-- character turns least to face, 3 (VTank's default) by turning within
	-- TargetSelectAngleRange and by distance beyond it.
	TargetSelectMethod = 3,
	TargetSelectAngleRange = 10 / 240, -- VTank publishes no default; this is the plugin's
	-- Debuffs before attacks: 1 debuffs the target and attacks it (VTank's default), 2 debuffs
	-- every monster of the target's priority in range first, 3 every monster in range first.
	DebuffEachFirst = 1,
	-- A debuff is cast again this long before it would run out (its spell's duration, where the
	-- client knows it; else DebuffRecastSeconds after it landed). VTank publishes no default.
	DebuffPrecastSeconds = 15,
	-- Melee swings at the power that makes a weapon of two attacks (a thrust and a slash) strike
	-- with the monster's damage type, else at AttackPower (VTank's default is on).
	AutoAttackPower = true,
	-- With Recklessness trained, melee and missile swings stay within its band of the power bar
	-- (20% to 80%), where it adds to the damage.
	UseRecklessness = false,
	-- Rings (a monster row's R column): a ring is cast when MinimumRingTargets monsters or more
	-- are within RingDistance, for a row that attacks too; for one that does not, when any monster
	-- of a row marked for rings is. VTank publishes no default for RingDistance; this is the
	-- plugin's.
	RingDistance = 6 / 240,
	MinimumRingTargets = 4,
	-- Arcs in place of bolts, where an arc is known at the bolt's level, at monsters ArcRange away
	-- or more (0: at any range).
	UseArcs = false,
	ArcRange = 0,
	-- A monster its row fights with a weapon is debuffed with a caster first, then the weapon is
	-- taken up again.
	SwitchWandsToDebuff = false,
	-- Between its casts and shots, the character side-steps, left and right in turn, so the bolts
	-- aimed where it stood miss (VTank's "spell dodge mode").
	DoJiggle = false,
	-- With a vulnerability of level 7 or more on the character: a dispel item used (Gems of
	-- Stillness, dispel potions and gems, Runes of Dispel, Black Potions), or Eradicate Life Magic
	-- Self cast.
	UseDispelItems = false,
	CastDispelSelf = false,
	IdlePeaceMode = false, -- peace mode when not fighting
	BlacklistMonsterTimeoutSeconds = 30, -- how long a monster given up on is left alone
	-- Attacks that fail on a monster before it is left alone (VTank publishes no default; three
	-- was this plugin's own count).
	BlacklistMonsterAttemptCount = 3,
	-- A monster that is not there: casts at it that the client or the server refuses as having no
	-- such target, GhostMonsterSpellAttemptCount times running, mark it a ghost; with the health
	-- tracker, a target whose health has not changed in GhostDeleteHPTrackerSeconds of fighting
	-- is one too. A ghost is left alone until the client lets it go (a plugin cannot make the
	-- client forget an object). VTank publishes none of their defaults.
	DeleteGhostMonsters = true,
	GhostMonsterSpellAttemptCount = 4,
	DeleteGhostMonstersByHPTracker = false,
	GhostDeleteHPTrackerSeconds = 30,
	StopMacroOnDeath = true,

	-- Navigation.
	NavCloseStopRange = 1.5 / 240, -- how close counts as reaching a waypoint, and as following
	-- Following a character (a follow route): its exact way round corners, or straight at it.
	-- VTank publishes no default; this is the plugin's.
	FollowAroundCorners = false,
	-- Closed doors in the way are opened when within DoorOpenRange of it; a locked one is looked at
	-- (appraised) from within DoorIDRange and picked with a lockpick the Consumables allow when the
	-- character's lockpicking beats the lock by DoorLockpickDiffExcessThreshold. VTank publishes no
	-- defaults; these are the plugin's.
	OpenDoors = true,
	DoorOpenRange = 1.5 / 240,
	DoorIDRange = 20 / 240,
	DoorLockpickDiffExcessThreshold = 0,
	-- Navigation is turned off once the character is further than this from every waypoint of its
	-- route (VTank's default since it made the setting all but unlimited).
	NavFarStopRange = 999999,
	NavPriorityBoost = false, -- walk the route before fighting
	LootPriorityBoost = false, -- loot corpses before fighting the next monster

	-- Looting.
	["CorpseApproachRange-Max"] = 20 / 240, -- corpses nearer than this are walked to and looted
	-- How near a corpse the character walks before opening it.
	["CorpseApproachRange-Min"] = 1.5 / 240,
	-- A corpse that did not open is tried again after CorpseOpenTimeoutSeconds; after
	-- BlacklistCorpseOpenAttemptCount tries (VTank's default) it is left for
	-- BlacklistCorpseOpenTimeoutSeconds.
	CorpseOpenTimeoutSeconds = 10,
	BlacklistCorpseOpenAttemptCount = 30,
	BlacklistCorpseOpenTimeoutSeconds = 600,
	-- How long an open corpse's items are waited for, and their appraisals.
	CorpseItemAppearanceTimeoutSeconds = 5,
	CorpseItemIDTimeoutSeconds = 5,
	-- How long a kill's corpse is remembered for looting.
	CorpseCacheTimeoutMinutes = 2,
	-- Tries at taking one item before it is left on the corpse (VTank's default).
	CorpseLootItemMaxAttempts = 20,
	-- Scrolls of spells the character does not know and can learn (its buffed skill within 15 of
	-- the spell's difficulty) are looted and read, unless the profile keeps them.
	ReadUnknownScrolls = true,
	-- Other corpses: those of the fellows' kills (LootFellowCorpses), those of kills neither the
	-- character's nor its fellows' (LootAllCorpses); with LootOnlyRareCorpses only a corpse that
	-- generated a rare is looted. VTank publishes no defaults; these leave others' corpses alone.
	LootAllCorpses = false,
	LootFellowCorpses = false,
	LootOnlyRareCorpses = false,
	-- Salvage bags are combined as the loot profile's ranges say.
	CombineSalvage = true,
	-- Stacks merged toward the bottom-most of their kind, so the top-most is the one partial stack
	-- (AutoStack); the main pack's items moved into the side packs (AutoCram). Off unless turned on.
	AutoStack = false,
	AutoCram = false,

	-- Vitals: during a fight, recharge below these percentages; with no monster in range, top off
	-- to these.
	["Recharge-Norm-HitP"] = 50,
	["Recharge-Norm-Stam"] = 20,
	["Recharge-Norm-Mana"] = 20,
	["Recharge-NoTarg-HitP"] = 80,
	["Recharge-NoTarg-Stam"] = 60,
	["Recharge-NoTarg-Mana"] = 60,
	-- Kits are used in magic stance too (VTank's UseKitsInMagicMode).
	UseKitsInMagicMode = true,
	-- The character drops to peace mode before using a healing kit.
	GoToPeaceModeToUseKits = false,
	-- After switching to magic stance to recharge, the marks are raised by RechargeBoostAmount
	-- points of the vital for up to RechargeBoostTimeSeconds; with ClearLevelBoostFlagOnCast, only
	-- until a spell of another kind is cast. VTank publishes no defaults; these give no boost.
	RechargeBoostTimeSeconds = 0,
	RechargeBoostAmount = 0,
	ClearLevelBoostFlagOnCast = false,
	-- A healing kit is used when the heal is at least this likely to land (percent).
	MinimumHealKitSuccessChance = 70,
	-- Stamina to Health and Mana to Health are cast when they give this many times the health a
	-- mana of a heal gives (counting the stamina or mana they cost to get back).
	StaminaToHealthMultiplier = 1.5,
	ManaToHealthMultiplier = 2,
	-- Helping fellows (VTank's DoHelp): a fellow (of the fellowship, or of the Tank's own fellow
	-- on this machine, PeerFellow) under a helper mark and within its distance is healed, given
	-- stamina or given mana; with UseHealersHeart a Healer's Heart heals, and with
	-- JumpOutWandCasting the character jumps as soon as it is used. VTank publishes no defaults;
	-- these are the plugin's.
	DoHelp = false,
	["Recharge-Helper-HitP"] = 60,
	["Recharge-Helper-Stam"] = 30,
	["Recharge-Helper-Mana"] = 30,
	HelperDistanceHitP = 25 / 240,
	HelperDistanceStam = 25 / 240,
	HelperDistanceMana = 25 / 240,
	UseHealersHeart = false,
	JumpOutWandCasting = false,
	-- As a fellowship's leader, recruits whoever tells it "xp" and answers VTank's other fellow
	-- commands (line, remove, leader, startvote, vote, help).
	AutoFellowManagement = false,
	-- Summoning: a combat pet from the pets the Pets setting names (empty: any of the character's
	-- mastery) once PetMonsterDensity monsters a row summons for are within the pet range
	-- (PetRangeMode 0: AttackDistance; 1: PetCustomRange). VTank publishes no defaults; these are
	-- the plugin's.
	SummonPets = false,
	PetRangeMode = 0,
	PetCustomRange = 15 / 240,
	PetMonsterDensity = 1,
	-- Mana stones and charges keep worn items filled while the macro is stopped too.
	ManaChargesWhenOff = false,
	-- Worn items are refilled from mana stones and charges when one is under this percent of its
	-- mana (VTank's RefillWornMana, RefillWornMana-Item-ManaPercent).
	RefillWornMana = true,
	["RefillWornMana-Item-ManaPercent"] = 25,
	-- Looting keeps up to this many mana stones and fills empty ones from looted items the profile
	-- does not keep that hold at least ManaTankMinimumMana (VTank's own two options).
	ManaStoneLootCount = 3,
	ManaTankMinimumMana = 500,
	-- The kits, foods, potions, mana stones and charges the macro may use, by name, comma separated
	-- (VTank's Consumables tab; this plugin's own reading: empty means any of them the pack holds).
	Consumables = "",

	-- Buffing. Recast a buff with less than RebuffTimeRemainingSeconds left; with IdleBuffTopoff,
	-- when there is nothing to fight, from IdleBuffTopoffTimeSeconds.
	RebuffTimeRemainingSeconds = 300,
	IdleBuffTopoff = false,
	IdleBuffTopoffTimeSeconds = 1200,
	BuffSpells = "", -- the self buffs to keep up, by family, comma separated (this plugin's own; empty: VTank's defaults)
	-- Buffs added to the default set, by family, comma separated (VTank's Buffs tab: a skill not
	-- trained, such as Leadership).
	AddedBuffs = "",
	-- The protections and banes cast (VTank's buff profiles): All, None, B, BPS, BPSA, ALFC, or
	-- Custom, the letters (B bludgeon, P pierce, S slash, A acid, L lightning, F fire, C cold) then
	-- in BuffProfile-Prots and BuffProfile-Banes.
	BuffProfile_Prots = "All",
	BuffProfile_Banes = "All",
	["BuffProfile-Prots"] = "BPSALFC",
	["BuffProfile-Banes"] = "BPSALFC",
	-- The buffs never cast, by family, comma separated (VTank's Buffs tab's exclusions).
	ExcludedBuffs = "",
	-- Spell components whose spells are never cast, comma separated; a scarab's name leaves out
	-- every spell of its level (VTank's scarab exclusions).
	BlacklistedSpellComps = "",
	-- The items never buffed, by name, comma separated (VTank's Items tab's Add (No Buffs)).
	NoBuffItems = "",
	-- After a buff is cast, the other buffs are recast BuffCastRecast_Seconds earlier for
	-- BuffCastRecastReset_Seconds, so buffs running low together are cast together. VTank
	-- publishes no defaults; none here.
	BuffCastRecast_Seconds = 0,
	BuffCastRecastReset_Seconds = 0,
	-- Now and then, a protection on a player nearby, whoever and whichever at random.
	RandomHelperBuffs = false,
	RandomHelperIntervalSeconds = 60,
	BuffItems = true, -- buff the weapon held and the armour worn (this plugin's own)
	-- How long an item buff is counted to last where the client does not know its spell's
	-- duration (this plugin's own). An item buff's time runs while the character plays, as the
	-- character's age counts it, not while it is logged out.
	ItemBuffMinutes = 25,

	-- Debuffs: a debuff is cast again this long after it landed (VTank recasts before it runs out).
	DebuffRecastSeconds = 60,
	MaxSpellLevel = 8, -- the highest level of spell the macro casts (this plugin's own)
	-- A spell is cast when the buffed skill of its school is at least its difficulty and this much
	-- more: for buffs, and for hunting (attacks, debuffs, and the restoring spells, which are cast
	-- when mana is low). VTank's names and defaults.
	["SpellDiffExcessThreshold-Buff"] = 5,
	["SpellDiffExcessThreshold-Hunt"] = 25,

	-- This plugin's own.
	-- After turning to cast at a monster well off to the side, the macro looks again and fights
	-- another first if one is now to be (VTank's breakable turn, under this plugin's name).
	BreakableTurn = false,
	-- Buffs are cast moving backward, which casts them sooner (VTank's fast-cast buffs, under this
	-- plugin's name).
	FastCastBuffs = false,
	Offhand = "", -- the shield or offhand weapon taken up beside a melee weapon, where a monster's row names none
	Weapons = "", -- the weapons a monster row's `auto` weapon is chosen from, by name, comma separated (VTank's Items tab; empty: every weapon in the pack)
	Pets = "", -- the summoning devices summoned from, by name, comma separated (VTank's Items tab's pets; empty: any the pack holds of the character's mastery)
	PeerFellow = "", -- the name of the Tank's own fellow over this machine's clients (VTank's VI2 fellow); empty: none
	AttackPower = 60, -- percent of the power bar for a swing or shot
	TiredAttackPower = 10, -- the power when stamina is under Recharge-Norm-Stam
	GiveUpSeconds = 60, -- a monster not killed in this long is left alone a while
	StuckTries = 3, -- recoveries tried at one waypoint before it is skipped
	NavJourneys = true, -- journeys (portals and recalls too) to a far route, a hunting place, round a stuck waypoint
	-- A route's legs as VTank walks them: each straight at its waypoint, round a wall only once
	-- stuck. Off, a leg whose waypoint is out of sight follows the client's route there from the
	-- start, round corners and through doors, and a monster behind a wall is walked to the same way.
	NavStraightLegs = false,
	JourneyToRouteMetres = 500, -- a route further than this at the start is travelled to by a journey
	LogOff = "", -- the log's types left out, comma separated (VTank's /vt log <type> off)
	HealSpells = "", -- heal spells, by id, comma separated, tried before kits
	UseHealingKits = true,
	RestWhenIdle = true, -- with nothing to recharge with, wait in peace mode for stamina and mana
	MinFreeSlots = 2, -- fewer free pack slots than this is a full pack
	-- stop, recall (then stop), vendor (the vendor trip, then on; stopping if still full), or keep
	-- (fight on, looting nothing more)
	OnFullPack = "stop",
	-- Trading at a vendor (this plugin's own): the loot profile's and the vendor's own profile's
	-- Sell rules sold, and the buy list ("Prismatic Taper=50, Bread=5") and the vendor profile's
	-- Keep # and Keep rules bought.
	VendorSell = true,
	VendorBuy = true,
	BuyList = "",
	Vendor = "", -- the vendor the vendor trip goes to, by name (set from the selection)
	-- Buffs from a buffbot (this plugin's own): the buffbot's character, by name (set from the
	-- selection); the tell that asks it; the trip to it when a buff it gave has under
	-- BuffbotMinutes left or is gone (0: only when told, /tank buffbot); the longest wait there for
	-- the buffs, seconds; and how long after a trip that brought none it asks again, minutes.
	Buffbot = "",
	BuffbotRequest = "buffs",
	BuffbotMinutes = 10,
	BuffbotWaitSeconds = 300,
	BuffbotRetryMinutes = 10,
	-- After a death that left a corpse, back to it and everything in it taken again (this
	-- plugin's own).
	RecoverCorpse = true,
	StopAfterDeaths = 0, -- stop after this many deaths in one run (0: never)
	RecallSpell = 0, -- the spell the macro recalls with (0: none)
	StopAfterMinutes = 0, -- stop after running this long (0: never)
	StartOnLogin = false, -- start the macro when this character enters the world
	RouteRecordMetres = 5, -- recording a route while walking, a point each time the character has gone this far
	Weapon = "", -- the weapon to fight with, by name (else whatever is wielded, else the first in the pack)
	CasterWeapon = "", -- the caster taken up to cast buffs and other spells on oneself (else the first in the pack)
	Ammo = "", -- the arrows, bolts or darts readied for a missile weapon (else the first in the pack)
	Route = "", -- the route file in the routes folder
	LootProfile = "", -- the loot profile file in the profiles folder
	Meta = "", -- the VTank meta (.met) to run with the macro, from the metas folder
	-- VTank's remote control, a small bar on the screen: Force Buff, its cancel, follow the
	-- selected, and the follow-around-corners and target-lock switches.
	MiniRemote = false,
	-- VTank's hotkeys, each a key with Ctrl or Alt ("Ctrl+Shift+P"; empty: none): run or pause the
	-- macro, the melee attack height, loot the open container by the loot profile; and the meta
	-- states bound to keys ("Hunt=Ctrl+1, Vendor=Alt+V").
	HotkeyRun = "",
	HotkeyHigh = "",
	HotkeyMid = "",
	HotkeyLow = "",
	HotkeyLootContainer = "",
	MetaStateHotkeys = "",
}

-- Lower-cased name -> the name as written above.
local canonical = {}
for k in pairs(S.defaults) do
	canonical[string.lower(k)] = k
end

-- VTank's options page spells one option two ways.
canonical["switchwandtodebuff"] = "SwitchWandsToDebuff"

function S.name(name)
	return canonical[string.lower(tostring(name))]
end

-- VTank's options (its Options List, 125 names), which a meta or a settings profile may set. One
-- this macro has not got is kept for the run as it was given, so a meta reads back what it set,
-- and does nothing here.
S.VTANK = {}
for name in string.gmatch([[
EnableLooting EnableNav EnableBuffing EnableCombat SpellDiffExcessThreshold-Hunt
SpellDiffExcessThreshold-Buff ArrowheadFletchDiffExcessThreshold Recharge-Norm-HitP Recharge-Norm-Stam
Recharge-Norm-Mana Recharge-NoTarg-HitP Recharge-NoTarg-Stam Recharge-NoTarg-Mana Recharge-Helper-HitP
Recharge-Helper-Stam Recharge-Helper-Mana DoHelp AttackDistance AttackMinimumDistance ApproachDistance
RingDistance CorpseApproachRange-Max CorpseApproachRange-Min NavCloseStopRange NavFarStopRange
UsePortalDistance HelperDistanceHitP HelperDistanceStam HelperDistanceMana MinimumRingTargets
DefaultMeleeAttackHeight CastDispelSelf UseDispelItems AutoCram AutoStack ReadUnknownScrolls
UseDispelDrum SwitchWandsToDebuff AutoCraftItems UseHealersHeart JumpOutWandCasting LootAllCorpses
LootFellowCorpses DoJiggle RandomHelperBuffs RandomHelperIntervalSeconds IdlePeaceMode TargetLock
StopMacroOnDeath UseArcs ArcRange TargetSelectMethod TargetSelectAngleRange IdleBuffTopoff
IdleBuffTopoffTimeSeconds RebuffTimeRemainingSeconds RefillWornMana RefillWornMana-Item-ManaPercent
BuffProfile-Prots BuffProfile-Banes BuffProfile_Prots BuffProfile_Banes DebuffEachFirst AutoAttackPower
LootPriorityBoost CorpseCacheTimeoutMinutes CorpseItemAppearanceTimeoutSeconds CorpseItemIDTimeoutSeconds
DebuffSelectionMethod ManaStoneLootCount ManaTankMinimumMana SplitPeas SpellCompMin-Critical
SpellCompMin-Normal SpellCompMin-Idle RechargeBoostTimeSeconds RechargeBoostAmount UseSpecialAmmo
OpenDoors DoorIDRange DoorOpenRange DoorLockpickDiffExcessThreshold ManaChargesWhenOff
AutoFellowManagement MinimumHealKitSuccessChance UseKitsInMagicMode StaminaToHealthMultiplier
ManaToHealthMultiplier NavPriorityBoost DeleteGhostMonsters GhostMonsterSpellAttemptCount
WhoYouGonnaCall BlacklistMonsterAttemptCount BlacklistMonsterTimeoutSeconds CombineSalvage
LootOnlyRareCorpses DeleteGhostMonstersByHPTracker GhostDeleteHPTrackerSeconds GoToPeaceModeToUseKits
UseRecklessness DebuffPrecastSeconds ClearLevelBoostFlagOnCast IdleCraftCount_HealthKits
IdleCraftCount_StamKits IdleCraftCount_ManaKits IdleCraftCount_HealthFood IdleCraftCount_StamFood
IdleCraftCount_ManaFood BuffCastRecast_Seconds BuffCastRecastReset_Seconds EnableMeta
BlacklistedSpellComps DropToPeaceModeRetryCount FollowAroundCorners BlacklistCorpseOpenAttemptCount
BlacklistCorpseOpenTimeoutSeconds SummonPets PetRangeMode PetCustomRange PetRefillCount-Idle
PetRefillCount-Normal CorpseOpenTimeoutSeconds RechargeHandlerSet
]], "%S+") do
	S.VTANK[string.lower(name)] = name
end

-- VTank's options this macro has not got, as a meta or a profile set them this run: lower-cased
-- name -> { name, value }.
S.extra = {}

S.values = {}
for k, v in pairs(S.defaults) do
	S.values[k] = v
end

-- The character's settings over the defaults.
function S.load()
	local saved = store.character.tank_settings or {}
	S.extra = {}
	for k, v in pairs(S.defaults) do
		S.values[k] = v
	end
	for k, v in pairs(saved) do
		local n = S.name(k)
		if n ~= nil and type(v) == type(S.defaults[n]) then
			S.values[n] = v
		end
	end
end

function S.save()
	local out = {}
	for k, v in pairs(S.values) do
		if v ~= S.defaults[k] then
			out[k] = v
		end
	end
	store.character.tank_settings = out
end

function S.get(name)
	local n = S.name(name)
	if n == nil then
		local e = S.extra[string.lower(tostring(name))]
		return e and e.value
	end
	return S.values[n]
end

-- Whether `name` is one of VTank's options this macro has not got.
function S.vtank_only(name)
	return S.name(name) == nil and S.VTANK[string.lower(tostring(name))] ~= nil
end

-- A distance setting in metres.
function S.metres(name)
	return (S.get(name) or 0) * S.UNIT
end

-- A percentage setting as a share.
function S.share(name)
	return (S.get(name) or 0) / 100
end

-- Set one setting from a value or the text typed for it; answers nil and why when it cannot.
-- `save = false` changes it for this run only (a meta's change is not kept).
function S.set(name, value, save)
	local n = S.name(name)
	if n == nil then
		local vt = S.VTANK[string.lower(tostring(name))]
		if vt == nil then
			return nil, "no setting " .. tostring(name)
		end
		-- One of VTank's this macro has not got: kept for the run, read back as it was given.
		S.extra[string.lower(vt)] = { name = vt, value = value }
		return true, "kept, though this macro has no " .. vt
	end
	local kind = type(S.defaults[n])
	if type(value) == "string" and kind ~= "string" then
		if kind == "number" then
			value = tonumber(value)
		elseif kind == "boolean" then
			local v = string.lower(value)
			if v == "true" or v == "on" or v == "yes" or v == "1" then
				value = true
			elseif v == "false" or v == "off" or v == "no" or v == "0" then
				value = false
			else
				value = nil
			end
		end
	elseif type(value) == "number" and kind == "boolean" then
		value = value ~= 0
	end
	if type(value) ~= kind then
		return nil, n .. " takes a " .. kind
	end
	S.values[n] = value
	if save ~= false then
		S.save()
	end
	return true
end

-- Every setting name, sorted without case.
function S.names()
	local out = {}
	for k in pairs(S.defaults) do
		table.insert(out, k)
	end
	table.sort(out, function(a, b)
		return string.lower(a) < string.lower(b)
	end)
	return out
end

-- A list setting of names ("Strength Self, Armor Self"), each trimmed.
function S.list(name)
	local out = {}
	for part in string.gmatch(tostring(S.get(name) or ""), "[^,]+") do
		local p = string.match(part, "^%s*(.-)%s*$")
		if p ~= "" then
			table.insert(out, p)
		end
	end
	return out
end

-- Whether a list setting names `value`, without case.
function S.listed(name, value)
	local want = string.lower(tostring(value or ""))
	for _, p in ipairs(S.list(name)) do
		if string.lower(p) == want then
			return true
		end
	end
	return false
end

-- A list setting of numbers ("1, 2, 3").
function S.numbers(name)
	local out = {}
	for id in string.gmatch(S.get(name) or "", "%d+") do
		table.insert(out, tonumber(id))
	end
	return out
end

return S
