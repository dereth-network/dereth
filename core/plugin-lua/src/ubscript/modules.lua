-- `dereth.ubscript.modules`: UtilityBelt's other modules a script can `require` (`acclient`,
-- `filesystem`, `ubnet`, `http`, `dats`), `game.Messages`, and the commands of VTank and
-- UtilityBelt a script types with `game.Actions.InvokeChat`, each over the plugin interface.

return function(U)
	local D, class, state, norm, list, num, id_of, wo = U.D, U.class, U.state, U.norm, U.list, U.num, U.id_of, U.wo

	-- Coordinates ----------------------------------------------------------------------------------

	-- A place as UtilityBelt's coordinates: map coordinates (`NS`, `EW`, `Z`) or a landcell and an
	-- offset in it.
	local Coordinates = class("Coordinates")
	local function coords(at)
		return Coordinates.new({ at = at })
	end
	U.coords = coords
	local function place(c)
		local at = state(c).at
		if at.ns ~= nil and at.cell == nil then
			return { ns = at.ns, ew = at.ew }
		end
		return at
	end
	Coordinates.define({
		get = {
			NS = function(c)
				return state(c).at.ns or 0
			end,
			EW = function(c)
				return state(c).at.ew or 0
			end,
			Z = function(c)
				return state(c).at.z or 0
			end,
			LandCell = function(c)
				return state(c).at.cell or 0
			end,
			LocalX = function(c)
				return state(c).at.x or 0
			end,
			LocalY = function(c)
				return state(c).at.y or 0
			end,
			LocalZ = function(c)
				return state(c).at.z or 0
			end,
		},
		call = {
			DistanceTo = function(c, o)
				return U.distance(state(c).at, state(o).at, false)
			end,
			DistanceToFlat = function(c, o)
				return U.distance(state(c).at, state(o).at, true)
			end,
			HeadingTo = function(c, o)
				local a, b = state(c).at, state(o).at
				if a.ns ~= nil and b.ns ~= nil then
					local h = math.deg(math.atan2(b.ew - a.ew, b.ns - a.ns))
					return (h + 360) % 360
				end
				return 0
			end,
			ToString = function(c)
				return U.coords_text(state(c).at)
			end,
		},
	})
	Coordinates.tostring = function(c)
		return U.coords_text(state(c).at)
	end
	local CoordinatesStatic = U.static("Coordinates", {
		new = function(a, b, c, d)
			if d ~= nil then
				return coords({ cell = num(a), x = num(b), y = num(c), z = num(d) })
			elseif a ~= nil then
				return coords({ ns = num(a), ew = num(b), z = num(c) or 0 })
			end
			return coords({ ns = 0, ew = 0, z = 0 })
		end,
	})

	-- acclient -------------------------------------------------------------------------------------

	-- An action of the movement and combat modules: queued as the script's other actions are.
	local function queued(name, fields, run, options, cb)
		if U.ACTION_SPECS[name] == nil then
			U.ACTION_SPECS[name] = {
				name = name,
				type = 12000,
				run = function(a)
					return state(a).run(a)
				end,
			}
		end
		local a = U.new_action(name, options)
		state(a).run = run
		for k, v in pairs(fields) do
			state(a).fields[norm(k)] = v
		end
		if type(cb) == "function" then
			table.insert(state(a).callbacks, cb)
		end
		U.queue_add(a)
		return a
	end

	-- The movements a script holds: the plugin interface holds a movement control for at most ten
	-- seconds at a time, so a motion turned on is held again while it stays on.
	local MOTIONS = {
		forward = "forward",
		backward = "backward",
		strafeleft = "strafe_left",
		straferight = "strafe_right",
		turnleft = "turn_left",
		turnright = "turn_right",
	}
	local held = {}
	local function motion_name(m)
		if type(m) == "string" then
			return MOTIONS[norm(m)]
		end
		local by_number = { [0] = "forward", "backward", "turn_left", "turn_right", "strafe_left", "strafe_right" }
		return by_number[num(m)]
	end

	local Movement = class("ACMovement")
	Movement.define({
		get = {
			Heading = function()
				return D.game.player.heading or 0
			end,
			MyPhysicsCoordinates = function()
				return coords(D.game.player.position or {})
			end,
		},
		call = {
			GetPhysicsCoordinates = function(_, id)
				local h = D.game.world:get(id_of(id))
				return h and h.position and coords(h.position) or nil
			end,
			SetHeading = function(_, degrees, accuracy, options, cb)
				return queued("ACSetHeadingAction", { Degrees = degrees, Accuracy = accuracy }, function(a)
					return U.call(a, D.nav.face, num(state(a).fields.degrees) or 0)
				end, options, cb)
			end,
			RunStraightTo = function(_, c, accuracy, options, cb)
				return queued("ACRunStraightToAction", { Coordinates = c, Accuracy = accuracy }, function(a)
					return U.call(a, D.nav.move_to, place(state(a).fields.coordinates), num(state(a).fields.accuracy) or 1)
				end, options, cb)
			end,
			WalkStraightTo = function(_, c, accuracy, options, cb)
				return queued("ACWalkStraightToAction", { Coordinates = c, Accuracy = accuracy }, function(a)
					return U.call(a, D.nav.move_to, place(state(a).fields.coordinates), num(state(a).fields.accuracy) or 1)
				end, options, cb)
			end,
			SetMotion = function(_, m, on)
				local kind = motion_name(m)
				if kind == nil then
					error(U.refusal("SetMotion(" .. tostring(m) .. ")", "the plugin interface holds forward, backward, turns and strafes"), 2)
				end
				if on then
					held[kind] = true
					if kind == "forward" then
						D.nav.autorun(true)
					else
						D.nav.motion(kind, 10000)
					end
				else
					held[kind] = nil
					if kind == "forward" then
						D.nav.autorun(false)
					else
						D.nav.stop()
					end
				end
			end,
			GetMotion = function(_, m)
				return held[motion_name(m) or ""] == true
			end,
			ClearAllMotions = function()
				local any = next(held) ~= nil
				held = {}
				if any then
					D.nav.stop()
					pcall(D.nav.autorun, false)
				end
				return true
			end,
			Dispose = function() end,
		},
	})

	local Combat = class("ACCombat")
	local attack_power, attack_height = 0.5, "medium"
	local HEIGHTS = { [1] = "high", [2] = "medium", [3] = "low" }
	Combat.define({
		get = {
			RepeatAttacks = function()
				local o = D.game.player.options or {}
				return o.auto_repeat_attack == true
			end,
			AttackInProgress = function()
				return D.game.player.busy == true
			end,
			AttackPower = function()
				return attack_power
			end,
			AttackHeight = function()
				for n, h in pairs(HEIGHTS) do
					if h == attack_height then
						return n
					end
				end
				return 2
			end,
		},
		set = {
			AttackPower = function(_, v)
				attack_power = math.max(0, math.min(1, tonumber(num(v)) or 0.5))
			end,
			AttackHeight = function(_, v)
				attack_height = HEIGHTS[num(v)] or "medium"
			end,
		},
		call = {
			IsAttackable = function(_, id)
				local h = D.game.world:get(id_of(id))
				return h ~= nil and h.class == "Monster"
			end,
			Attack = function(_, id, options, cb)
				return queued("ACAttackAction", { ObjectId = id_of(id) }, function(a)
					return U.call(a, D.combat.attack, state(a).fields.objectid, { power = attack_power, height = attack_height })
				end, options, cb)
			end,
			CancelAttack = function(_, options, cb)
				return queued("ACCancelAttackAction", {}, function(a)
					return U.call(a, D.combat.stop)
				end, options, cb)
			end,
			Dispose = function() end,
		},
	})

	local Client = class("ACClientControls")
	Client.define({
		call = {
			DistanceTo3D = function(_, a, b)
				return U.distance(U.place_of(id_of(a)), U.place_of(id_of(b)), false)
			end,
			DistanceToPlayer = function(_, id)
				return U.distance(U.place_of(D.game.player.id), U.place_of(id_of(id)), false)
			end,
			AddChatText = function(_, text)
				D.chat.print(tostring(text))
			end,
			RequestHealthUpdate = function(_, id)
				pcall(D.act.appraise, id_of(id))
			end,
			GetPhysicsCoordinates = function(_, id)
				local h = D.game.world:get(id_of(id))
				return h and h.position and coords(h.position) or nil
			end,
			Quit = function()
				D.session.quit()
			end,
			Dispose = function() end,
		},
	})

	-- Marks in the world: a D3D object is a world marker (or a line, ring or text) of the plugin's,
	-- shown while `Visible`.
	local D3D = class("DecalD3DObj")
	local SHAPES = { HorizontalArrow = 0, VerticalArrow = 1, Ring = 2, Cylinder = 3, Sphere = 4, Cube = 5, TiltedCube = 6 }
	local shape_names = {}
	for k, v in pairs(SHAPES) do
		shape_names[v] = k
	end
	local function argb(c)
		c = num(c) or 0xFFFFFFFF
		return string.format("#%06X", c % 16777216)
	end
	local function redraw(o)
		local s = state(o)
		if s.mark ~= nil then
			pcall(s.mark.remove, s.mark)
			s.mark = nil
		end
		if not s.visible or s.anchor == nil then
			return
		end
		local anchor = s.anchor
		local where = anchor.object and { object = anchor.object } or { position = anchor.position }
		local ok, m
		if s.text ~= nil then
			where.label = s.text
			where.color = argb(s.text_color or s.color)
			where.height = anchor.height or 1
			ok, m = pcall(D.draw.text, where)
		elseif shape_names[s.shape] == "Ring" then
			where.radius = math.max(0.2, (s.scale or 1) * 1)
			where.color = argb(s.color)
			ok, m = pcall(D.draw.ring, where)
		else
			where.color = argb(s.color)
			where.icon = s.icon
			ok, m = pcall(D.markers.world.add, D.markers.world, where)
		end
		if ok then
			s.mark = m
		end
	end
	local function d3d()
		local o = D3D.new({ visible = false, color = 0xFFFFFFFF, scale = 1 })
		U.at_end(function()
			local s = state(o)
			if s.mark ~= nil then
				pcall(s.mark.remove, s.mark)
			end
		end)
		return o
	end
	local function setter(field)
		return function(o, v)
			state(o)[field] = v
			redraw(o)
		end
	end
	local function getter(field, default)
		return function(o)
			local v = state(o)[field]
			if v == nil then
				return default
			end
			return v
		end
	end
	D3D.define({
		get = {
			Visible = getter("visible", false),
			Color = getter("color", 0xFFFFFFFF),
			Color2 = getter("color2", 0xFFFFFFFF),
			Autoscale = getter("autoscale", false),
			HBounce = getter("hbounce", 0),
			PBounce = getter("pbounce", 0),
			ScaleX = getter("scalex", 1),
			ScaleY = getter("scaley", 1),
			ScaleZ = getter("scalez", 1),
		},
		set = {
			Visible = function(o, v)
				state(o).visible = v == true
				redraw(o)
			end,
			Color = setter("color"),
			Color2 = setter("color2"),
			Autoscale = setter("autoscale"),
			HBounce = setter("hbounce"),
			PBounce = setter("pbounce"),
			ScaleX = setter("scalex"),
			ScaleY = setter("scaley"),
			ScaleZ = setter("scalez"),
		},
		call = {
			Scale = function(o, f)
				state(o).scale = num(f) or 1
				redraw(o)
			end,
			OrientToCamera = function() end,
			OrientToCoords = function() end,
			OrientToObject = function() end,
			OrientToPlayer = function() end,
			Anchor = function(o, a, b, c, d, e)
				if d ~= nil or (b ~= nil and c == nil) or (type(a) == "number" and a > 0xFFFF and b ~= nil and c ~= nil and d ~= nil) then
					state(o).anchor = { object = id_of(a), height = num(b) }
				elseif c ~= nil then
					state(o).anchor = { position = { ns = num(a), ew = num(b) }, height = 0 }
				else
					state(o).anchor = { object = id_of(a), height = num(b) or 0 }
				end
				redraw(o)
			end,
			SetText = function(o, a, b, c, d)
				local text, color = a, c
				if type(a) ~= "string" then
					text, color = b, d
				end
				state(o).text = tostring(text or "")
				state(o).text_color = color
				redraw(o)
			end,
			SetIcon = function(o, icon)
				state(o).icon = num(icon)
				state(o).text = nil
				redraw(o)
			end,
			SetShape = function(o, s)
				state(o).shape = num(s)
				state(o).text = nil
				redraw(o)
			end,
			Dispose = function(o)
				local s = state(o)
				s.visible = false
				redraw(o)
			end,
		},
	})

	local DecalD3DClass = class("ACDecalD3D")
	local function marked(where)
		local o = d3d()
		state(o).anchor = where
		state(o).visible = true
		return o
	end
	DecalD3DClass.define({
		call = {
			NewD3DObj = function()
				return d3d()
			end,
			PointToObject = function(_, id, color)
				pcall(D.markers.arrow.set, D.markers.arrow, { object = id_of(id), color = argb(color) })
				local o = d3d()
				return o
			end,
			PointToCoords = function(_, lat, lng, z, color)
				pcall(D.markers.arrow.set, D.markers.arrow, { position = { ns = num(lat), ew = num(lng) }, color = argb(color) })
				return d3d()
			end,
			MarkObjectWithShape = function(_, id, shape, color)
				local o = marked({ object = id_of(id) })
				state(o).shape, state(o).color = num(shape), color
				redraw(o)
				return o
			end,
			MarkCoordsWithShape = function(_, lat, lng, z, shape, color)
				local o = marked({ position = { ns = num(lat), ew = num(lng) } })
				state(o).shape, state(o).color = num(shape), color
				redraw(o)
				return o
			end,
			MarkObjectWithIcon = function(_, id, icon)
				local o = marked({ object = id_of(id) })
				state(o).icon = num(icon)
				redraw(o)
				return o
			end,
			MarkCoordsWithIcon = function(_, lat, lng, z, icon)
				local o = marked({ position = { ns = num(lat), ew = num(lng) } })
				state(o).icon = num(icon)
				redraw(o)
				return o
			end,
			MarkObjectWith2DText = function(_, id, text, font, color)
				local o = marked({ object = id_of(id), height = 1 })
				state(o).text, state(o).text_color = tostring(text), color
				redraw(o)
				return o
			end,
			MarkObjectWith3DText = function(_, id, text, font, color)
				local o = marked({ object = id_of(id), height = 1 })
				state(o).text, state(o).text_color = tostring(text), color
				redraw(o)
				return o
			end,
			MarkCoordsWith2DText = function(_, lat, lng, z, text, color)
				local o = marked({ position = { ns = num(lat), ew = num(lng) }, height = 1 })
				state(o).text, state(o).text_color = tostring(text), color
				redraw(o)
				return o
			end,
			MarkCoordsWith3DText = function(_, lat, lng, z, text, color)
				local o = marked({ position = { ns = num(lat), ew = num(lng) }, height = 1 })
				state(o).text, state(o).text_color = tostring(text), color
				redraw(o)
				return o
			end,
			ColorToVec4 = function(_, c)
				c = num(c) or 0
				return U.Vector4(
					bit32.band(bit32.rshift(c, 16), 0xFF) / 255,
					bit32.band(bit32.rshift(c, 8), 0xFF) / 255,
					bit32.band(c, 0xFF) / 255,
					bit32.band(bit32.rshift(c, 24), 0xFF) / 255
				)
			end,
			Vec4ToColor = function(_, v)
				local x, y, z, w = U.vec(v)
				local function b(f)
					return math.max(0, math.min(255, math.floor(f * 255 + 0.5)))
				end
				return b(w) * 16777216 + b(x) * 65536 + b(y) * 256 + b(z)
			end,
			Dispose = function() end,
		},
		refused = {
			SetObjectTransparency = "a plugin does not change how the game draws an object",
			PlayObjectScript = "a plugin does not play the game's effects",
		},
	})

	local function shapes_enum()
		local byname = {}
		for k, v in pairs(SHAPES) do
			byname[norm(k)] = v
		end
		return setmetatable({}, {
			__index = function(_, k)
				return byname[norm(tostring(k))]
			end,
		})
	end
	U.modules["acclient"] = function()
		return {
			Movement = Movement.new({}),
			DecalD3D = DecalD3DClass.new({}),
			Combat = Combat.new({}),
			Client = Client.new({}),
			Coordinates = CoordinatesStatic,
			DecalD3DShape = shapes_enum(),
			ACMotion = setmetatable({}, {
				__index = function(_, k)
					return k
				end,
			}),
		}
	end
	U.globals.Coordinates = CoordinatesStatic

	-- filesystem -----------------------------------------------------------------------------------

	-- The script's folder (read only, its package) and its data folder (the plugin's own), as
	-- UtilityBelt's sandboxed file access reads and writes them.
	local Access = class("FileSystemAccess")
	local function clean(path)
		path = string.gsub(tostring(path or ""), "\\", "/")
		path = string.gsub(path, "^/+", "")
		return path
	end
	local function under(dir, f)
		f = clean(f)
		if dir == "" then
			return f
		end
		return dir .. "/" .. f
	end
	local function listing(names, dir, files)
		local out, seen = {}, {}
		local prefix = dir == "" and "" or (dir .. "/")
		for _, n in ipairs(names) do
			if string.sub(n, 1, #prefix) == prefix then
				local rest = string.sub(n, #prefix + 1)
				local slash = string.find(rest, "/", 1, true)
				if files and slash == nil then
					table.insert(out, rest)
				elseif not files and slash ~= nil then
					local d = string.sub(rest, 1, slash - 1)
					if not seen[d] then
						seen[d] = true
						table.insert(out, d)
					end
				end
			end
		end
		table.sort(out)
		return list(out)
	end
	Access.define({
		call = {
			ReadText = function(a, f)
				local s = state(a)
				local p = clean(f)
				if s.data then
					return D.files.data(p)
				end
				return D.files.package(p)
			end,
			ReadLines = function(a, f)
				local text = Access.call[norm("ReadText")](a, f)
				local out = {}
				for line in string.gmatch((text or "") .. "\n", "([^\n]*)\n") do
					table.insert(out, (string.gsub(line, "\r$", "")))
				end
				if #out > 0 and out[#out] == "" then
					table.remove(out)
				end
				return list(out)
			end,
			WriteText = function(a, f, contents)
				if not state(a).data then
					error(U.refusal("WriteText", "the script's own folder is read only; write to filesystem.GetData()"), 2)
				end
				return D.files.save_data(clean(f), tostring(contents or ""))
			end,
			AppendText = function(a, f, contents)
				if not state(a).data then
					error(U.refusal("AppendText", "the script's own folder is read only; write to filesystem.GetData()"), 2)
				end
				local p = clean(f)
				return D.files.save_data(p, (D.files.data(p) or "") .. tostring(contents or ""))
			end,
			FileExists = function(a, f)
				local p = clean(f)
				if state(a).data then
					return D.files.data(p) ~= nil
				end
				return D.files.package(p) ~= nil
			end,
			DirectoryExists = function(a, d)
				local dir = clean(d)
				local names = state(a).data and D.files.data_list() or D.files.package_list()
				for _, n in ipairs(names or {}) do
					if string.sub(n, 1, #dir + 1) == dir .. "/" then
						return true
					end
				end
				return dir == ""
			end,
			GetFiles = function(a, d)
				local names = state(a).data and D.files.data_list() or D.files.package_list()
				return listing(names or {}, clean(d), true)
			end,
			GetDirectories = function(a, d)
				local names = state(a).data and D.files.data_list() or D.files.package_list()
				return listing(names or {}, clean(d), false)
			end,
			CreateDirectory = function()
				return true
			end,
			IsValidFilePath = function(_, f)
				local p = clean(f)
				return p ~= "" and not string.find(p, "%.%.")
			end,
			IsValidDirectoryPath = function(_, d)
				return not string.find(clean(d), "%.%.")
			end,
			CanWriteFile = function(a)
				return state(a).data == true
			end,
		},
		refused = {
			DeleteFile = "a plugin's data folder keeps what it saves; save the file empty instead",
			DeleteDirectory = "a plugin's data folder keeps what it saves",
		},
	})
	local FileSystem = class("FileSystem")
	FileSystem.define({
		call = {
			GetScript = function()
				return Access.new({ data = false })
			end,
			GetData = function()
				return Access.new({ data = true })
			end,
		},
		refused = {
			GetCustom = "a plugin reaches only its own package and its own data folder",
		},
	})
	U.modules["filesystem"] = function()
		return FileSystem.new({})
	end

	-- http -------------------------------------------------------------------------------------------

	-- An HTTPS request, through the plugin interface's fetch: to the domains a manifest names.
	local Http = class("SocketHttpModule")
	Http.define({
		call = {
			Request = function(_, url, body, headers, options, cb)
				return queued("HttpRequestAction", { Url = url }, function(a)
					local req = { url = tostring(url) }
					if body ~= nil then
						req.method = "POST"
						req.body = tostring(body)
					end
					if type(headers) == "table" then
						req.headers = headers
					end
					local e, d, again = U.call(a, D.net.fetch, req)
					if e == nil then
						local v = state(a).value or {}
						state(a).fields.response = U.args("HttpResponseData", {
							StatusCode = v.status or 0,
							Body = v.body or "",
							Headers = v.headers or {},
						})
					end
					return e, d, again
				end, options, cb)
			end,
		},
	})
	U.modules["http"] = function()
		return Http.new({})
	end

	-- ubnet ----------------------------------------------------------------------------------------

	-- UtilityBelt's network between one's own clients: here the plugin's copies in the player's
	-- other clients on this machine (the plugin interface's peers), a channel's messages heard by
	-- the script with the same name in each.
	local UBNet = class("UBNetModule")
	local subscriptions = {}
	local peers_listening = false
	local clients = {}
	local function ensure_peers()
		if peers_listening then
			return
		end
		peers_listening = true
		U.listen("peers.message", function(m)
			clients[m.from] = true
			local subs = subscriptions[m.channel]
			if subs == nil then
				return
			end
			for _, fn in ipairs(table.clone(subs)) do
				U.spawn(fn, m.channel, m.data)
			end
		end)
		U.at_end(function()
			subscriptions = {}
			peers_listening = false
		end)
	end
	local ClientInfo = class("UBNetClientInfo")
	ClientInfo.define({
		get = {
			Name = function(c)
				return state(c).name
			end,
			CharacterName = function(c)
				return state(c).name
			end,
		},
	})
	UBNet.define({
		call = {
			SubscribeToChannel = function(_, channel, fn)
				ensure_peers()
				local c = tostring(channel)
				subscriptions[c] = subscriptions[c] or {}
				table.insert(subscriptions[c], fn)
			end,
			UnsubscribeFromChannel = function(_, channel, fn)
				local subs = subscriptions[tostring(channel)] or {}
				for i, f in ipairs(subs) do
					if f == fn then
						table.remove(subs, i)
						return
					end
				end
			end,
			PublishToChannel = function(_, channel, message)
				D.peers.send(tostring(channel), message)
			end,
			GetClients = function()
				ensure_peers()
				local out = {}
				for name in pairs(clients) do
					table.insert(out, ClientInfo.new({ name = name }))
				end
				return list(out)
			end,
			Dispose = function() end,
		},
		refused = {
			RemoteExec = "a script's code never crosses to another client; publish a message on a channel instead",
		},
	})
	U.modules["ubnet"] = function()
		return UBNet.new({})
	end

	-- What a script can read of the game's data files through the plugin interface's own.
	U.modules["dats"] = function()
		error(U.refusal("dats", "a plugin reads the game's data through game.data (landblocks and dungeon cells)"), 2)
	end
	U.modules["utilitybelt.acmessages"] = function()
		error(U.refusal("utilitybelt.acmessages", "use game.Messages, the client's message stream"), 2)
	end
	U.modules["ace.datloader"] = U.modules["dats"]
	U.modules["ace.entity"] = U.modules["dats"]

	-- game.Messages ------------------------------------------------------------------------------

	-- The client's message stream, by message name: `game.Messages.Incoming.Item_SetAppraiseInfo`
	-- (and `.Outgoing.`) is an event whose argument holds the message's `Name`, `Opcode` and `Data`,
	-- its decoded fields under the plugin interface's names for them.
	local message_events = {}
	local function message_event(dir, name)
		local key = dir .. ":" .. name
		local e = message_events[key]
		if e ~= nil then
			return e
		end
		e = U.event(name, function()
			if not D.host.has("messages") then
				return
			end
			U.listen("messages." .. name, function(m)
				if m.dir == dir then
					U.fire(e, U.args(name, { Name = m.name, Opcode = m.opcode, Data = m.fields or {} }))
				end
			end)
		end)
		message_events[key] = e
		U.at_end(function()
			message_events[key] = nil
		end)
		return e
	end
	local function direction(dir)
		return setmetatable({}, {
			__index = function(_, name)
				return message_event(dir, tostring(name))
			end,
		})
	end
	function U.message_handler()
		return {
			Incoming = direction("received"),
			Outgoing = direction("sent"),
		}
	end

	-- Other plugins' commands -----------------------------------------------------------------------

	local TANK = "network.dereth.tank"
	local function service(name, args)
		local p = D.services.call(TANK, name, args)
		local ok, why = D.pawait(p)
		if ok then
			return nil
		end
		return "ServerError", "the Tank: " .. tostring(why), false
	end

	-- VTank's commands, answered by the bundled Tank where it has them.
	local function vtank(words)
		local verb = norm(words[1] or "")
		if verb == "opt" and norm(words[2] or "") == "set" and words[3] ~= nil then
			local value = table.concat(words, " ", 4)
			local v = value
			if string.lower(value) == "true" then
				v = true
			elseif string.lower(value) == "false" then
				v = false
			elseif tonumber(value) ~= nil then
				v = tonumber(value)
			end
			return service("set_setting", { name = words[3], value = v })
		elseif verb == "start" or verb == "stop" or verb == "pause" then
			return service("command", { line = verb })
		elseif verb == "nav" and norm(words[2] or "") == "load" and words[3] ~= nil then
			return service("command", { line = "route " .. table.concat(words, " ", 3) })
		elseif verb == "meta" and norm(words[2] or "") == "set" and words[3] ~= nil then
			return service("set_meta_state", { state = table.concat(words, " ", 3) })
		end
		return "UnknownCommand", U.refusal("/vt " .. table.concat(words, " "), "the Tank answers /vt opt set, start, stop, pause, nav load and meta set"), false
	end

	-- UtilityBelt's own commands that a script types, answered here.
	local function nearest_named(name, landscape)
		local best, bestd = nil, math.huge
		local me = D.game.player.id
		local lname = string.lower(name)
		for _, h in ipairs(D.game.world:find({ name_contains = name, sort = "distance" })) do
			if h.id ~= me and (not landscape or (h.container == nil and h.wielder == nil)) then
				local exact = string.lower(h.name or "") == lname
				local d = h:distance() or math.huge
				if exact then
					d = d - 1e6
				end
				if d < bestd then
					best, bestd = h, d
				end
			end
		end
		return best
	end
	local function ub(words, line)
		local verb = norm(words[1] or "")
		if verb == "mexec" then
			local text = string.match(line, "^%s*/ub%s+mexec%s+(.*)$") or table.concat(words, " ", 2)
			local ok, err = pcall(function()
				local expr = D.require("dereth.expr")
				expr.eval(text, expr.env({ vars = U.expr_vars }))
			end)
			if not ok then
				return "Exception", tostring(err), false
			end
			return nil
		elseif verb == "uselp" or verb == "use" then
			local target = nearest_named(table.concat(words, " ", 2), verb == "uselp")
			if target == nil then
				return "InvalidTargetObject", "nothing named " .. table.concat(words, " ", 2) .. " is near", false
			end
			return U.call(nil, D.act.use, target.id)
		elseif verb == "follow" then
			local target = nearest_named(table.concat(words, " ", 2), true)
			if target == nil then
				return "InvalidTargetObject", "no one named " .. table.concat(words, " ", 2) .. " is near", false
			end
			D.nav.follow(target.id)
			return nil
		elseif verb == "givep" or verb == "give" then
			local rest = table.concat(words, " ", 2)
			local item, to = string.match(rest, "^(.-)%s+to%s+(.+)$")
			if item == nil then
				return "UnknownCommand", "/ub give <item> to <target>", false
			end
			local mine = D.game.world:find({ container = D.game.player.id, name_contains = item })
			local target = nearest_named(to, true)
			if mine[1] == nil or target == nil then
				return "InvalidTargetObject", "no " .. item .. " held, or no " .. to .. " near", false
			end
			return U.call(nil, D.items.give, mine[1].id, target.id)
		elseif verb == "bc" then
			local rest = string.match(line, "^%s*/ub%s+bc%s+(.*)$") or ""
			D.peers.send("ub.bc", rest)
			return U.invoke_chat(nil, rest)
		end
		return "UnknownCommand", U.refusal("/ub " .. table.concat(words, " "), "here, /ub answers mexec, use, uselp, follow, give and bc"), false
	end

	U.expr_vars = {}
	-- A line another client's copy of the script broadcast (`/ub bc`) is typed here too.
	table.insert(U.on_start, function()
		if not D.host.has("peers.local") then
			return
		end
		U.listen("peers.message", function(m)
			if m.channel == "ub.bc" and type(m.data) == "string" then
				U.spawn(U.invoke_chat, nil, m.data)
			end
		end)
	end)
	function U.route_chat(line)
		local verb, rest = string.match(line, "^[/@](%S+)%s*(.*)$")
		if verb == nil then
			return false
		end
		local words = {}
		for w in string.gmatch(rest, "%S+") do
			table.insert(words, w)
		end
		local v = string.lower(verb)
		if v == "vt" then
			return true, vtank(words)
		elseif v == "vto" then
			table.insert(words, 1, "set")
			table.insert(words, 1, "opt")
			return true, vtank(words)
		elseif v == "vtns" then
			return true, vtank({ "stop" })
		elseif v == "ub" or v == "ubs" then
			return true, ub(words, line)
		end
		return false
	end
end
