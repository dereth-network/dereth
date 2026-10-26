-- The experience tracker: experience and luminance earned this session, per hour since the first
-- gain, per hour over the last five minutes, and the time to the next level at the session's
-- rate. Times are the plugin's own clock (os.clock), so a replay gives the same rates.

local M = {}

local WINDOW = 300

local function fresh()
	return { earned = 0, started = nil, gains = {} }
end

M.xp = fresh()
M.lum = fresh()

function M.reset()
	M.xp = fresh()
	M.lum = fresh()
end

-- A gain of `n` now.
local function gain(t, n)
	if n <= 0 then
		return
	end
	local now = os.clock()
	t.started = t.started or now
	t.earned = t.earned + n
	table.insert(t.gains, { at = now, n = n })
	-- Only the last five minutes' gains are kept.
	while #t.gains > 0 and now - t.gains[1].at > WINDOW do
		table.remove(t.gains, 1)
	end
end

function M.gain_xp(n)
	gain(M.xp, n)
end

function M.gain_lum(n)
	gain(M.lum, n)
end

-- The rate per hour since the first gain.
function M.rate(t)
	if t.started == nil then
		return 0
	end
	local secs = os.clock() - t.started
	return secs > 0 and t.earned * 3600 / secs or 0
end

-- The rate per hour over the last five minutes (or the session, when it is shorter).
function M.recent(t)
	if t.started == nil then
		return 0
	end
	local now = os.clock()
	local sum = 0
	for _, g in ipairs(t.gains) do
		if now - g.at <= WINDOW then
			sum = sum + g.n
		end
	end
	local span = math.min(WINDOW, now - t.started)
	return span > 0 and sum * 3600 / span or 0
end

-- Seconds to the next level at the session's rate, or nil when unknown.
function M.to_level()
	local xp = game.player.xp
	local rate = M.rate(M.xp)
	if xp == nil or xp.to_level == nil or rate <= 0 then
		return nil
	end
	return xp.to_level * 3600 / rate
end

return M
