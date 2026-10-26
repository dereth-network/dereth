-- Restocking: the character buys its own components and mana potions again, and keeps to its
-- post.
--
-- Its post is where it stood when it was first turned on (or where `/buffbot post` was typed),
-- kept with the character. Between players, a character more than POST_REACH metres from its post
-- (a client that lost its connection on the way, a death) walks back to it.
--
-- The Restock setting lists what to keep and where it is sold: `Prismatic Taper=200@Archmage
-- Cindrue, Mana Potion=20@Thelnoth Cort the Healer` (an item, how many to keep, and the vendor
-- that sells it; a name may hold a comma). An item is due when the character holds fewer than
-- RestockAt percent of its count. Between players, with the line empty (or with players waiting
-- when a listed item has run out), the character walks to each vendor it needs, buys each due item
-- up to its count (no more than its pyreals pay for), and walks back to its post (else to where it
-- stood), facing the way it faced there. Requests that come meanwhile wait in line. A trip that bought nothing is tried
-- again only after RETRY seconds.

local R = {}
local B = nil

-- How long after a trip that bought nothing another is tried, seconds.
local RETRY = 600
-- How long a vendor's wares are waited for once it is used, seconds.
local WARES_WAIT = 5

-- How far from its post the character may stand, metres; and how long after a walk back that did
-- not get there another is tried, seconds.
local POST_REACH, POST_RETRY = 5, 60

local retry_at = nil
local post_retry_at = nil

function R.init(bot)
	B = bot
end

-- The restock list: { name, count, vendor } each, as the Restock setting writes it.
function R.list()
	local out = {}
	local text = tostring(B.get("Restock") or "")
	for name, count, vendor in string.gmatch(text, "%s*(.-)%s*=%s*(%d+)%s*@%s*([^,]+),?") do
		vendor = string.match(vendor, "^(.-)%s*$")
		if name ~= "" and vendor ~= "" then
			table.insert(out, { name = name, count = tonumber(count), vendor = vendor })
		end
	end
	return out
end

-- The post: { cell, x, y, z, heading }, or nil.
function R.post()
	return store.character.buffbot_post
end

-- The post is where the character stands now; answers the line to tell.
function R.set_post()
	local p = game.player.position
	if p == nil or p.cell == nil then
		return "no place to keep as the post"
	end
	store.character.buffbot_post = { cell = p.cell, x = p.x, y = p.y, z = p.z, heading = p.heading }
	return string.format("its post is here (%08X %.1f, %.1f)", p.cell, p.x or 0, p.y or 0)
end

-- A place's point on the map, metres: its landblock's corner and its offset.
local function global(p)
	if p == nil or p.cell == nil then
		return nil
	end
	local bx = bit32.rshift(p.cell, 24)
	local by = bit32.band(bit32.rshift(p.cell, 16), 0xFF)
	return bx * 192 + (p.x or 0), by * 192 + (p.y or 0), p.z or 0
end

-- Metres from the character to its post, or nil.
function R.from_post()
	local ax, ay, az = global(game.player.position)
	local bx, by, bz = global(R.post())
	if ax == nil or bx == nil then
		return nil
	end
	return math.sqrt((bx - ax) ^ 2 + (by - ay) ^ 2 + (bz - az) ^ 2)
end

-- How many of `name` the pack holds, stacks counted whole, without case.
function R.held(name)
	local want, n = string.lower(name), 0
	for _, it in ipairs(game.inventory.items()) do
		if string.lower(it.name or "") == want then
			n = n + (it.stack or 1)
		end
	end
	return n
end

-- The listed items due: under RestockAt percent of their count. Answers them, and whether one has
-- run out.
function R.due()
	local due, out = {}, false
	for _, e in ipairs(R.list()) do
		local held = R.held(e.name)
		if held < e.count * B.get("RestockAt") / 100 then
			e.held = held
			table.insert(due, e)
			out = out or held == 0
		end
	end
	return due, out
end

-- Wait up to `secs` for `cond()`, a quarter second at a time; answers whether it came true.
local function wait_for(cond, secs)
	local until_t = os.clock() + secs
	while os.clock() < until_t do
		if cond() then
			return true
		end
		task.sleep(250)
	end
	return cond()
end

-- At `vendor` (its wares open), buy each of `items` up to its count. Answers what was bought, as
-- lines.
local function buy(items)
	local info = vendor.info()
	if info == nil then
		return {}
	end
	local coins = game.player_int(20) or 0
	local rows, before = {}, {}
	for _, r in ipairs(info.stock or {}) do
		for _, e in ipairs(items) do
			if string.lower(r.name or "") == string.lower(e.name) then
				local need = e.count - R.held(e.name)
				if r.amount ~= nil and r.amount >= 0 then
					need = math.min(need, r.amount)
				end
				local price = r.price or 0
				if price > 0 then
					need = math.min(need, math.floor(coins / price))
				end
				if need > 0 then
					table.insert(rows, { id = r.id, count = need })
					before[e.name] = R.held(e.name)
					coins = coins - need * price
				end
			end
		end
	end
	local bought = {}
	if #rows > 0 then
		local ok, why = pawait(vendor.buy(rows))
		if not ok then
			B.note("restocking: buying: %s", tostring(why))
		end
		for _, e in ipairs(items) do
			local got = R.held(e.name) - (before[e.name] or R.held(e.name))
			if got > 0 then
				table.insert(bought, string.format("%d %s", got, e.name))
			end
		end
	end
	return bought
end

-- To `where` (a place or an object) by a walking journey. Answers ok, why.
local function go(where)
	return pawait(nav.path_to(where, { portals = false, recalls = false }))
end

-- Back to the post, facing the way it faced there. Answers ok, why.
local function to_post(post)
	local ok, why = go({ cell = post.cell, x = post.x, y = post.y, z = post.z })
	if ok and post.heading ~= nil then
		pawait(nav.face(post.heading))
	end
	return ok, why
end

-- The trip: to each vendor of `due`, buying there, and back to the post.
local function trip(due)
	local p = game.player.position
	if p == nil then
		return
	end
	local post = R.post() or { cell = p.cell, x = p.x, y = p.y, z = p.z, heading = p.heading }
	local names = {}
	for _, e in ipairs(due) do
		table.insert(names, string.format("%s (%d of %d)", e.name, e.held, e.count))
	end
	B.note("restocking: %s", table.concat(names, ", "))
	B.restocking = true
	if B.on_change then
		B.on_change()
	end
	local first = B.queue[1]
	if first ~= nil then
		B.tell(first.name, "I am buying components and will be back shortly; you are next.")
	end
	local vendors, order = {}, {}
	for _, e in ipairs(due) do
		if vendors[e.vendor] == nil then
			vendors[e.vendor] = {}
			table.insert(order, e.vendor)
		end
		table.insert(vendors[e.vendor], e)
	end
	local all = {}
	for _, name in ipairs(order) do
		local v = game.world:first({ name = name, sort = "distance" })
		if v == nil then
			B.note("restocking: %s is not in sight", name)
		else
			B.state = "Restocking: to " .. name
			local ok, why = go(v)
			if not ok then
				B.note("restocking: could not get to %s (%s)", name, tostring(why))
			else
				pawait(act.use(v.id))
				if wait_for(function()
					return vendor.info() ~= nil
				end, WARES_WAIT) then
					local bought = buy(vendors[name])
					B.note("restocking: at %s: bought %s", name, #bought > 0 and table.concat(bought, ", ") or "nothing")
					for _, b in ipairs(bought) do
						table.insert(all, b)
					end
					pawait(vendor.close())
				else
					B.note("restocking: %s's wares did not open", name)
				end
			end
		end
	end
	B.state = "Restocking: back to its post"
	local ok, why = to_post(post)
	B.note(ok and "restocking: back at its post" or "restocking: could not get back (%s)", tostring(why))
	B.restocking = false
	if #all == 0 then
		retry_at = os.clock() + RETRY
	end
	if B.on_change then
		B.on_change()
	end
end

-- One check between players: a trip when something is due and the line is empty, or something
-- has run out. `now` forces one whatever is due (the player's /buffbot restock).
function R.step(now)
	local d = R.post() and R.from_post()
	if d ~= nil and d > POST_REACH and (post_retry_at == nil or os.clock() >= post_retry_at) then
		B.note("back to its post (%d m away)", math.floor(d))
		B.state = "Back to its post"
		local ok, why = to_post(R.post())
		B.note(ok and "back at its post" or "could not get back to its post (%s)", tostring(why))
		post_retry_at = (not ok) and (os.clock() + POST_RETRY) or nil
		return
	end
	if not now and retry_at ~= nil and os.clock() < retry_at then
		return
	end
	local due, out = R.due()
	if now and #due == 0 then
		for _, e in ipairs(R.list()) do
			e.held = R.held(e.name)
			if e.held < e.count then
				table.insert(due, e)
			end
		end
	end
	if #due == 0 then
		return
	end
	if not now and #B.queue > 0 and not out then
		return
	end
	retry_at = nil
	trip(due)
end

return R
