-- What to do with each monster, as VTank's monster list says it: rows of a name (or an expression)
-- and how to fight what matches. The first row after the default that matches a monster decides;
-- when none does, the default row does.
--
-- A row: { name, priority, attack, ring, streak, damage, weapon, offhand, exvuln,
-- debuffs = { imperil = true, vuln = true, ... } }. priority -1 means leave it alone; 0 to 4 fight
-- it, higher first. attack false means debuff only (unless ring or streak is on: VTank's R and S
-- columns, rings when the crowd for one is about, streaks in place of bolts). damage is a damage
-- type, "auto", or a mode: "drain", "harm", "fists" (Tusker Fists) or "random"; weapon the name of
-- the weapon or caster to use on it, or "auto" (the best of the Weapons setting's for it); offhand
-- the shield or offhand weapon taken up beside a melee weapon.

local X = require("dereth.expr")
local S = require("settings")
local names = require("names")

local M = {}

M.DEFAULT = "<DEFAULT>"

-- The variables a monster expression may name, as VTank's are (its /vt listmonstervariables):
-- the monster's name, its kind's number, its species' name, its maximum health (from an
-- appraisal, asked for the first time an expression needs it), its range in metres, whether it
-- wields armour (a shield), a setting's value (`setting_<name>`), and the meta's state.
M.VARIABLES = { "true", "false", "name", "typeid", "species", "maxhp", "range", "hasshield", "setting_x", "metastate" }
local VARS = { "name", "typeid", "species", "maxhp", "range", "hasshield", "metastate" }

-- The rows as last read or saved (a store read carries the whole table, so they are kept).
local cache = nil
-- The monsters whose appraisal was asked for, for their maximum health.
local appraising = {}

-- Forget the kept rows (the character changed).
function M.forget()
	cache = nil
	appraising = {}
end

function M.rows()
	if cache ~= nil then
		return cache
	end
	-- Before a character is in the world there is no character store: the default row only.
	local ok, rows = pcall(function()
		return store.character.tank_monsters
	end)
	rows = ok and rows or nil
	if ok and rows ~= nil and #rows > 0 then
		cache = rows
	end
	if rows == nil or #rows == 0 then
		rows = { { name = M.DEFAULT, priority = 1, attack = true, damage = "auto", weapon = "", debuffs = {} } }
	end
	return rows
end

function M.save(rows)
	store.character.tank_monsters = rows
	cache = rows
end

-- The row named `name` (without case), and its index.
function M.find(name)
	for i, r in ipairs(M.rows()) do
		if string.lower(r.name) == string.lower(name) then
			return r, i
		end
	end
	return nil
end

-- Set fields of the row named `name`, adding it (before nothing else: after the rows there) when
-- it is new.
function M.set(name, fields)
	local rows = M.rows()
	local row = nil
	for _, r in ipairs(rows) do
		if string.lower(r.name) == string.lower(name) then
			row = r
		end
	end
	if row == nil then
		row = { name = name, priority = 1, attack = true, damage = "auto", weapon = "", debuffs = {} }
		table.insert(rows, row)
	end
	for k, v in pairs(fields) do
		row[k] = v
	end
	M.save(rows)
	return row
end

function M.remove(name)
	local rows = M.rows()
	for i, r in ipairs(rows) do
		if string.lower(r.name) == string.lower(name) and r.name ~= M.DEFAULT then
			table.remove(rows, i)
			M.save(rows)
			return true
		end
	end
	return false
end

-- Whether a row's name is an expression rather than a monster's name.
local function is_expression(text)
	return string.find(text, "[=<>#&|!]") ~= nil
end

local compiled = {}

-- Monster `o`'s appraisal, asked for once, when an expression needs what only it tells.
local function appraise(o)
	if not o.appraised and not appraising[o.id] then
		appraising[o.id] = true
		act.appraise(o.id)
	end
end

-- The species' name of monster `o` (its integer property 2, from its appraisal), or "".
local SPECIES = 2
function M.species(o)
	local n = o:int(SPECIES)
	if n == nil then
		appraise(o)
	end
	return names.SPECIES[n or 0] or ""
end

-- The maximum health of monster `o`, from its appraisal; 0 until that has come.
function M.maxhp(o)
	local h = o:health()
	if h ~= nil and h.max ~= nil then
		return h.max
	end
	appraise(o)
	return 0
end

-- Whether monster `o` wields armour (a shield, as VTank's `hasshield` reads it).
function M.hasshield(o)
	return #game.world:find({ wielder = o.id, class = "Armor" }) > 0
end

-- The values of the variables `uses` names (all when nil), for monster `o`.
function M.variables(o, meta_state, uses)
	local vars = {}
	local function want(v)
		return uses == nil or uses[v]
	end
	vars.name = o.name or ""
	vars.typeid = o.wcid or 0
	if want("species") then
		vars.species = M.species(o)
	end
	if want("maxhp") then
		vars.maxhp = M.maxhp(o)
	end
	vars.range = o:distance() or 0
	if want("hasshield") then
		vars.hasshield = M.hasshield(o)
	end
	vars.metastate = meta_state or ""
	for _, setting in ipairs(uses and uses.settings or {}) do
		local v = S.get(setting)
		if v == nil then
			v = 0
		end
		vars["setting_" .. setting] = v
	end
	return vars
end

-- Whether expression `text` holds for monster `o`.
local function holds(text, o, meta_state)
	local c = compiled[text]
	if c == nil then
		-- VTank's monster expressions name their variables bare; the expression language reads a
		-- bare word as text, so each is written as a run variable first.
		local src = text
		local uses = { settings = {} }
		for _, v in ipairs(VARS) do
			local n
			src, n = string.gsub(src, "%f[%w_]" .. v .. "%f[^%w_]", "$" .. v)
			uses[v] = n > 0
		end
		src = string.gsub(src, "%f[%w_]setting_([%w_]+)", function(setting)
			table.insert(uses.settings, setting)
			return "$setting_" .. setting
		end)
		local ok, f = pcall(X.compile, src)
		c = { fn = ok and f or false, uses = uses }
		compiled[text] = c
	end
	if c.fn == false then
		return false
	end
	local ok, v = pcall(c.fn, X.env({ vars = M.variables(o, meta_state, c.uses) }))
	return ok and (v == true or (type(v) == "number" and v ~= 0))
end
-- The row deciding what to do with monster `o`.
function M.match(o, meta_state)
	local rows = M.rows()
	local default = rows[1]
	for _, r in ipairs(rows) do
		if r.name == M.DEFAULT then
			default = r
		elseif is_expression(r.name) then
			if holds(r.name, o, meta_state) then
				return r
			end
		elseif string.lower(r.name) == string.lower(o.name or "") then
			return r
		end
	end
	return default
end

return M
