-- `dereth.ubscript.views`: UtilityBelt's `utilitybelt.views` (its HUDs) and `imgui` modules over
-- the plugin interface's declared windows.
--
-- A script draws a HUD the immediate way: each time the HUD renders, its `OnRender` handlers call
-- `ImGui.Text`, `ImGui.Button` and the rest. Here each render builds the window's tree of nodes from
-- those calls; when the tree's shape is what it was, only the properties that changed are set on
-- the window, else the window is declared again. A click, a toggle, text typed, a tab chosen reach
-- the next render as ImGui reports them: the button's call answers true, the checkbox's answers
-- true and its new value. A HUD renders when its window is open: ten times a second, and at once
-- after the player acts on it. What a declared window cannot be (a free drawing at a pixel, a
-- color picker, the mouse and the keys) is refused or answers as though nothing happened there.

return function(U)
	local D, class, state, norm, list, num = U.D, U.class, U.state, U.norm, U.list, U.num

	-- ImGui's flag and enumeration tables, as Dear ImGui numbers them; a name not listed reads 0.
	local FLAGS = {
		ImGuiWindowFlags = {
			None = 0, NoTitleBar = 1, NoResize = 2, NoMove = 4, NoScrollbar = 8, NoScrollWithMouse = 16,
			NoCollapse = 32, AlwaysAutoResize = 64, NoBackground = 128, NoSavedSettings = 256,
			NoMouseInputs = 512, MenuBar = 1024, HorizontalScrollbar = 2048, NoFocusOnAppearing = 4096,
			NoBringToFrontOnFocus = 8192, AlwaysVerticalScrollbar = 16384,
			AlwaysHorizontalScrollbar = 32768, AlwaysUseWindowPadding = 65536, NoNavInputs = 262144,
			NoNavFocus = 524288, UnsavedDocument = 1048576, NoDocking = 2097152, NoNav = 786432,
			NoDecoration = 43, NoInputs = 786944,
		},
		ImGuiTableFlags = {
			None = 0, Resizable = 1, Reorderable = 2, Hideable = 4, Sortable = 8, NoSavedSettings = 16,
			ContextMenuInBody = 32, RowBg = 64, BordersInnerH = 128, BordersOuterH = 256,
			BordersInnerV = 512, BordersOuterV = 1024, BordersH = 384, BordersV = 1536, BordersInner = 640,
			BordersOuter = 1280, Borders = 1920, NoBordersInBody = 2048, NoBordersInBodyUntilResize = 4096,
			SizingFixedFit = 8192, SizingFixedSame = 16384, SizingStretchProp = 24576,
			SizingStretchSame = 32768, NoHostExtendX = 65536, NoHostExtendY = 131072,
			NoKeepColumnsVisible = 262144, PreciseWidths = 524288, NoClip = 1048576, PadOuterX = 2097152,
			NoPadOuterX = 4194304, NoPadInnerX = 8388608, ScrollX = 16777216, ScrollY = 33554432,
			SortMulti = 67108864, SortTristate = 134217728,
		},
		ImGuiTableColumnFlags = {
			None = 0, Disabled = 1, DefaultHide = 2, DefaultSort = 4, WidthStretch = 8, WidthFixed = 16,
			NoResize = 32, NoReorder = 64, NoHide = 128, NoClip = 256, NoSort = 512, NoSortAscending = 1024,
			NoSortDescending = 2048, NoHeaderLabel = 4096, NoHeaderWidth = 8192,
			PreferSortAscending = 16384, PreferSortDescending = 32768, IndentEnable = 65536,
			IndentDisable = 131072,
		},
		ImGuiTreeNodeFlags = {
			None = 0, Selected = 1, Framed = 2, AllowOverlap = 4, NoTreePushOnOpen = 8,
			NoAutoOpenOnLog = 16, DefaultOpen = 32, OpenOnDoubleClick = 64, OpenOnArrow = 128, Leaf = 256,
			Bullet = 512, FramePadding = 1024, SpanAvailWidth = 2048, SpanFullWidth = 4096,
			CollapsingHeader = 26,
		},
		ImGuiInputTextFlags = {
			None = 0, CharsDecimal = 1, CharsHexadecimal = 2, CharsUppercase = 4, CharsNoBlank = 8,
			AutoSelectAll = 16, EnterReturnsTrue = 32, CallbackCompletion = 64, CallbackHistory = 128,
			CallbackAlways = 256, CallbackCharFilter = 512, AllowTabInput = 1024,
			CtrlEnterForNewLine = 2048, NoHorizontalScroll = 4096, AlwaysOverwrite = 8192,
			ReadOnly = 16384, Password = 32768, NoUndoRedo = 65536, CharsScientific = 131072,
		},
		ImGuiSelectableFlags = {
			None = 0, DontClosePopups = 1, SpanAllColumns = 2, AllowDoubleClick = 4, Disabled = 8,
			AllowOverlap = 16,
		},
		ImGuiTabBarFlags = {
			None = 0, Reorderable = 1, AutoSelectNewTabs = 2, TabListPopupButton = 4,
			NoCloseWithMiddleMouseButton = 8, NoTabListScrollingButtons = 16, NoTooltip = 32,
			FittingPolicyResizeDown = 64, FittingPolicyScroll = 128,
		},
		ImGuiTabItemFlags = {
			None = 0, UnsavedDocument = 1, SetSelected = 2, NoCloseWithMiddleMouseButton = 4, NoPushId = 8,
			NoTooltip = 16, NoReorder = 32, Leading = 64, Trailing = 128,
		},
		ImGuiCond = { None = 0, Always = 1, Once = 2, FirstUseEver = 4, Appearing = 8 },
		ImGuiSortDirection = { None = 0, Ascending = 1, Descending = 2 },
		ImGuiComboFlags = {
			None = 0, PopupAlignLeft = 1, HeightSmall = 2, HeightRegular = 4, HeightLarge = 8,
			HeightLargest = 16, NoArrowButton = 32, NoPreview = 64,
		},
		ImGuiSliderFlags = { None = 0, AlwaysClamp = 16, Logarithmic = 32, NoRoundToFormat = 64, NoInput = 128 },
		ImGuiDir = { None = -1, Left = 0, Right = 1, Up = 2, Down = 3 },
		ImGuiMouseButton = { Left = 0, Right = 1, Middle = 2 },
		ImGuiCol = {
			Text = 0, TextDisabled = 1, WindowBg = 2, ChildBg = 3, PopupBg = 4, Border = 5, BorderShadow = 6,
			FrameBg = 7, FrameBgHovered = 8, FrameBgActive = 9, TitleBg = 10, TitleBgActive = 11,
			TitleBgCollapsed = 12, MenuBarBg = 13, ScrollbarBg = 14, ScrollbarGrab = 15,
			ScrollbarGrabHovered = 16, ScrollbarGrabActive = 17, CheckMark = 18, SliderGrab = 19,
			SliderGrabActive = 20, Button = 21, ButtonHovered = 22, ButtonActive = 23, Header = 24,
			HeaderHovered = 25, HeaderActive = 26, Separator = 27,
		},
		ImGuiStyleVar = {
			Alpha = 0, DisabledAlpha = 1, WindowPadding = 2, WindowRounding = 3, WindowBorderSize = 4,
			WindowMinSize = 5, WindowTitleAlign = 6, ChildRounding = 7, ChildBorderSize = 8,
			PopupRounding = 9, PopupBorderSize = 10, FramePadding = 11, FrameRounding = 12,
			FrameBorderSize = 13, ItemSpacing = 14, ItemInnerSpacing = 15, IndentSpacing = 16,
			CellPadding = 17, ScrollbarSize = 18, ScrollbarRounding = 19, GrabMinSize = 20,
			GrabRounding = 21, TabRounding = 22, ButtonTextAlign = 23, SelectableTextAlign = 24,
		},
	}
	local function flags_table(name, t)
		local byname = {}
		for k, v in pairs(t or {}) do
			byname[norm(k)] = v
		end
		return setmetatable({}, {
			__index = function(_, k)
				if type(k) ~= "string" then
					return nil
				end
				return byname[norm(k)] or 0
			end,
			__newindex = function()
				error(name .. " is read-only", 2)
			end,
		})
	end
	local flag_tables = {}
	local function flags(name)
		local t = flag_tables[name]
		if t == nil then
			t = flags_table(name, FLAGS[name])
			flag_tables[name] = t
		end
		return t
	end
	local function has(v, bit)
		return v ~= nil and bit32.band(num(v) % 4294967296, bit) ~= 0
	end

	-- Measures: ImGui speaks in pixels; a declared window in character cells and lines.
	local CELL, LINE = 7, 16

	-- The render under way ------------------------------------------------------------------------

	local B = nil -- { hud, root, stack, last, sameline, ids, seq, keys, tooltip, disabled, color }

	local function container()
		return B.stack[#B.stack]
	end

	local function text_of(label)
		label = tostring(label or "")
		local shown = string.match(label, "^(.-)##") or label
		return shown
	end

	-- The key a widget is remembered by across renders: its label's id part (after `##`, or the
	-- label itself) within the ids pushed, as ImGui keys its widgets.
	local function key_of(label)
		label = tostring(label or "")
		local id = string.match(label, "###(.*)$") or string.match(label, "##(.*)$") or label
		local k = table.concat(B.ids, "/") .. "/" .. id
		local n = (B.seen[k] or 0) + 1
		B.seen[k] = n
		if n > 1 then
			k = k .. "#" .. n
		end
		return k
	end

	local MAX_NODES = 1800
	local function new_node(kind, props, key)
		B.seq = B.seq + 1
		local node = { kind = kind, props = props or {}, children = {}, key = key, id = "n" .. B.seq }
		if key ~= nil then
			B.keys[node.id] = key
		end
		return node
	end

	-- Put a node where the next item goes: after the last one, or beside it after `SameLine`.
	local function place(node)
		if B.seq > MAX_NODES then
			return node
		end
		if B.tooltip ~= nil then
			-- Inside a tooltip, items are its text.
			if node.props.text ~= nil then
				table.insert(B.tooltip, tostring(node.props.text))
			end
			return node
		end
		if B.capture ~= nil then
			table.insert(B.capture, node)
			return node
		end
		local c = container()
		local kids = c.children
		if B.sameline and #kids > 0 then
			local prev = kids[#kids]
			if prev.sameline_row then
				table.insert(prev.children, node)
			else
				local row = new_node("row", {})
				row.sameline_row = true
				row.children = { prev, node }
				kids[#kids] = row
			end
		else
			table.insert(kids, node)
		end
		B.sameline = false
		B.last = node
		return node
	end

	local function pending(key)
		local p = state(B.hud).pending[key]
		if p ~= nil then
			state(B.hud).pending[key] = nil
		end
		return p
	end

	local function enabled()
		return B.disabled == 0
	end

	local function color_of(c)
		if c == nil then
			return nil
		end
		if type(c) == "number" then
			-- ImGui's packed colors are ABGR.
			local r = bit32.band(c, 0xFF)
			local g = bit32.band(bit32.rshift(c, 8), 0xFF)
			local b = bit32.band(bit32.rshift(c, 16), 0xFF)
			return string.format("#%02X%02X%02X", r, g, b)
		end
		local x, y, z = U.vec(c)
		local function byte(f)
			return math.max(0, math.min(255, math.floor(f * 255 + 0.5)))
		end
		return string.format("#%02X%02X%02X", byte(x), byte(y), byte(z))
	end

	local function clip(s)
		s = tostring(s or "")
		if #s > 4000 then
			s = string.sub(s, 1, 4000)
		end
		return s
	end

	local function label(text, extra)
		local props = { text = clip(text) }
		if B.color ~= nil then
			props.color = B.color
		end
		for k, v in pairs(extra or {}) do
			props[k] = v
		end
		return place(new_node("label", props))
	end

	-- ImGui -----------------------------------------------------------------------------------------

	local ImGui = {}
	local V2 = U.Vector2

	function ImGui.Text(text, ...)
		if select("#", ...) > 0 then
			local ok, s = pcall(string.format, tostring(text), ...)
			if ok then
				text = s
			end
		end
		label(text)
	end
	ImGui.TextUnformatted = function(text)
		label(text)
	end
	ImGui.TextWrapped = function(text)
		label(text)
	end
	function ImGui.TextColored(color, text)
		label(text, { color = color_of(color) })
	end
	function ImGui.TextDisabled(text)
		label(text, { color = "#808080" })
	end
	function ImGui.BulletText(text)
		label("- " .. tostring(text))
	end
	function ImGui.Bullet()
		label("-")
		B.sameline = true
	end
	function ImGui.LabelText(lbl, text)
		label(tostring(text) .. "  " .. text_of(lbl))
	end
	function ImGui.SeparatorText(text)
		place(new_node("separator", {}))
		label(text)
	end
	function ImGui.Separator()
		if container().kind ~= "row" then
			place(new_node("separator", {}))
		end
	end
	function ImGui.Spacing() end
	function ImGui.NewLine()
		B.sameline = false
	end
	function ImGui.Dummy() end
	function ImGui.SameLine()
		B.sameline = true
	end
	function ImGui.AlignTextToFramePadding() end
	function ImGui.Indent() end
	function ImGui.Unindent() end

	local function button(lbl, extra)
		local key = key_of(lbl)
		local props = { text = clip(text_of(lbl)) }
		if not enabled() then
			props.enabled = false
		end
		for k, v in pairs(extra or {}) do
			props[k] = v
		end
		place(new_node("button", props, key))
		local p = pending(key)
		local clicked = p ~= nil and p.clicked == true
		B.last_clicked = clicked
		return clicked
	end
	function ImGui.Button(lbl)
		return button(lbl)
	end
	ImGui.SmallButton = ImGui.Button
	function ImGui.ArrowButton(id, dir)
		local arrows = { [0] = "<", [1] = ">", [2] = "^", [3] = "v" }
		return button((arrows[num(dir)] or ">") .. "##" .. tostring(id))
	end
	function ImGui.InvisibleButton()
		return false
	end
	function ImGui.ImageButton(id, tex)
		return button(tostring(id))
	end
	function ImGui.TabItemButton(lbl)
		return button(lbl)
	end

	function ImGui.Checkbox(lbl, v)
		local key = key_of(lbl)
		local props = { text = clip(text_of(lbl)), value = v == true }
		if not enabled() then
			props.enabled = false
		end
		place(new_node("toggle", props, key))
		local p = pending(key)
		if p ~= nil and p.value ~= nil then
			return true, p.value == true
		end
		return false, v == true
	end
	function ImGui.CheckboxFlags(lbl, flagsv, bit)
		local on = bit32.band(num(flagsv) or 0, num(bit) or 0) ~= 0
		local changed, now = ImGui.Checkbox(lbl, on)
		if changed then
			if now then
				flagsv = bit32.bor(num(flagsv) or 0, num(bit) or 0)
			else
				flagsv = bit32.band(num(flagsv) or 0, bit32.bnot(num(bit) or 0))
			end
		end
		return changed, flagsv
	end
	function ImGui.RadioButton(lbl, active, value)
		if value ~= nil then
			local on = num(active) == num(value)
			local pressed = button((on and "(*) " or "( ) ") .. text_of(lbl) .. "##" .. tostring(lbl))
			if pressed then
				return true, value
			end
			return false, active
		end
		return button((active and "(*) " or "( ) ") .. text_of(lbl) .. "##" .. tostring(lbl))
	end

	local function input(kind, lbl, value, extra, convert)
		local key = key_of(lbl)
		local props = { value = value }
		if kind ~= "textbox" then
			props.text = clip(text_of(lbl))
		end
		if not enabled() then
			props.enabled = false
		end
		for k, v in pairs(extra or {}) do
			props[k] = v
		end
		place(new_node(kind, props, key))
		local p = pending(key)
		if p ~= nil and p.value ~= nil then
			local v = p.value
			if convert ~= nil then
				v = convert(v)
			end
			return true, v
		end
		return false, value
	end
	function ImGui.InputText(lbl, text, maxlen, flagsv)
		local ro = has(flagsv, 16384)
		return input("input", lbl, clip(text), ro and { enabled = false } or nil, function(v)
			v = tostring(v)
			if maxlen ~= nil and #v > num(maxlen) then
				v = string.sub(v, 1, num(maxlen))
			end
			return v
		end)
	end
	function ImGui.InputTextWithHint(lbl, hint, text, maxlen, flagsv)
		return input("input", lbl, clip(text), { placeholder = clip(hint) }, tostring)
	end
	function ImGui.InputTextMultiline(lbl, text, maxlen, size, flagsv)
		local _, h = U.vec(size)
		local lines = h and h > 0 and math.max(2, math.floor(h / LINE)) or 4
		return input("textbox", lbl, clip(text), { editable = not has(flagsv, 16384), height = lines }, tostring)
	end
	local function int_of(v)
		v = tonumber(v) or 0
		return v >= 0 and math.floor(v + 0.5) or -math.floor(-v + 0.5)
	end
	function ImGui.InputInt(lbl, v, step)
		return input("number", lbl, int_of(v), { step = num(step) or 1 }, int_of)
	end
	function ImGui.InputFloat(lbl, v, step)
		local s = num(step)
		if s == nil or s == 0 then
			s = 0.1
		end
		return input("number", lbl, tonumber(v) or 0, { step = s }, tonumber)
	end
	ImGui.InputDouble = ImGui.InputFloat
	function ImGui.SliderInt(lbl, v, min, max)
		return input("slider", lbl, int_of(v), { min = num(min) or 0, max = num(max) or 100, step = 1 }, int_of)
	end
	function ImGui.SliderFloat(lbl, v, min, max)
		local lo, hi = num(min) or 0, num(max) or 1
		return input("slider", lbl, tonumber(v) or 0, { min = lo, max = hi, step = (hi - lo) / 100 }, tonumber)
	end
	ImGui.SliderAngle = ImGui.SliderFloat
	function ImGui.DragInt(lbl, v, speed, min, max)
		local extra = { step = 1 }
		if num(min) ~= nil and num(max) ~= nil and num(max) > num(min) then
			extra.min, extra.max = num(min), num(max)
		end
		return input("number", lbl, int_of(v), extra, int_of)
	end
	function ImGui.DragFloat(lbl, v, speed, min, max)
		local extra = { step = num(speed) or 0.1 }
		if num(min) ~= nil and num(max) ~= nil and num(max) > num(min) then
			extra.min, extra.max = num(min), num(max)
		end
		return input("number", lbl, tonumber(v) or 0, extra, tonumber)
	end

	local function items_of(items)
		local out = {}
		if type(items) == "string" then
			for s in string.gmatch(items, "[^%z]+") do
				table.insert(out, s)
			end
		else
			for _, s in ipairs(items or {}) do
				table.insert(out, tostring(s))
			end
		end
		return out
	end
	function ImGui.Combo(lbl, current, items)
		local opts = items_of(items)
		local cur = int_of(current)
		if #opts == 0 then
			label(text_of(lbl))
			return false, cur
		end
		-- Two items of one name are told apart by their place.
		local seen, shown = {}, {}
		for i, o in ipairs(opts) do
			local s = o
			if seen[s] then
				s = s .. " (" .. i .. ")"
			end
			seen[s] = true
			shown[i] = s
		end
		local value = shown[cur + 1] or shown[1]
		local changed, v = input("choice", lbl, value, { options = shown })
		if changed then
			for i, s in ipairs(shown) do
				if s == v then
					return true, i - 1
				end
			end
		end
		return false, cur
	end

	-- A combo's or a list box's items are the selectables inside it.
	function ImGui.BeginCombo(lbl, preview)
		B.capture = {}
		B.combo = { label = lbl, preview = preview, kind = "choice" }
		return true
	end
	function ImGui.EndCombo()
		local combo, items = B.combo, B.capture
		B.capture, B.combo = nil, nil
		if combo == nil then
			return
		end
		local opts = {}
		for _, n in ipairs(items or {}) do
			if n.props.text ~= nil then
				table.insert(opts, n.props.text)
			end
		end
		local key = key_of(combo.label)
		if #opts == 0 then
			label(text_of(combo.label) .. " " .. tostring(combo.preview or ""))
			return
		end
		local value = tostring(combo.preview or "")
		local found = false
		for _, o in ipairs(opts) do
			if o == value then
				found = true
			end
		end
		if not found then
			value = opts[1]
		end
		place(new_node("choice", { text = clip(text_of(combo.label)), options = opts, value = value }, key))
		local p = pending(key)
		if p ~= nil and p.value ~= nil then
			state(B.hud).chosen[key] = p.value
		end
	end
	function ImGui.BeginListBox(lbl, size)
		B.capture = {}
		B.combo = { label = lbl, kind = "list", size = size }
		return true
	end
	function ImGui.EndListBox()
		local combo, items = B.combo, B.capture
		B.capture, B.combo = nil, nil
		if combo == nil then
			return
		end
		local rows, sel = {}, nil
		for i, n in ipairs(items or {}) do
			table.insert(rows, { clip(n.props.text or "") })
			if n.props.selected then
				sel = i
			end
		end
		local key = key_of(combo.label)
		local _, h = U.vec(combo.size)
		place(new_node("list", {
			columns = { text_of(combo.label) },
			rows = rows,
			selected = sel,
			height = h and h > 0 and math.max(2, math.floor(h / 18)) or math.min(8, math.max(1, #rows)),
		}, key))
		local p = pending(key)
		if p ~= nil and p.rows ~= nil and p.rows[1] ~= nil then
			local n = items[p.rows[1]]
			if n ~= nil then
				state(B.hud).chosen[key] = n.props.text
			end
		end
	end
	function ImGui.ListBox(lbl, current, items, count)
		local opts = items_of(items)
		local cur = int_of(current)
		local rows = {}
		for _, o in ipairs(opts) do
			table.insert(rows, { clip(o) })
		end
		local key = key_of(lbl)
		place(new_node("list", {
			columns = { text_of(lbl) },
			rows = rows,
			selected = opts[cur + 1] and (cur + 1) or nil,
			height = math.min(num(count) or 8, math.max(1, #rows)),
		}, key))
		local p = pending(key)
		if p ~= nil and p.rows ~= nil and p.rows[1] ~= nil then
			return true, p.rows[1] - 1
		end
		return false, cur
	end

	-- The choice a combo or a list box made on its last render, which its selectable answers.
	local function chosen_in(capture_label, text)
		local key = table.concat(B.ids, "/") .. "/" .. (string.match(tostring(capture_label), "##(.*)$") or tostring(capture_label))
		local c = state(B.hud).chosen[key]
		if c ~= nil and c == text then
			state(B.hud).chosen[key] = nil
			return true
		end
		return false
	end

	function ImGui.Selectable(lbl, selected)
		local text = clip(text_of(lbl))
		if B.capture ~= nil and B.combo ~= nil then
			place(new_node("label", { text = text, selected = selected == true }))
			local hit = chosen_in(B.combo.label, text)
			if hit then
				return true, true
			end
			return false, selected
		end
		local pressed = button((selected and "> " or "") .. text .. "##" .. tostring(lbl))
		if pressed then
			return true, not selected
		end
		return false, selected
	end
	function ImGui.MenuItem(lbl, shortcut, selected)
		return button(lbl)
	end

	function ImGui.ProgressBar(fraction, size, overlay)
		local f = tonumber(num(fraction)) or 0
		local props = { value = math.max(0, math.min(1, f)) * 100, max = 100 }
		if overlay ~= nil then
			props.text = clip(overlay)
		end
		place(new_node("meter", props))
	end

	function ImGui.Image(tex, size)
		local props = {}
		if type(tex) == "table" and rawget(tex, "icon") ~= nil then
			props.icon = rawget(tex, "icon")
		elseif type(tex) == "table" and rawget(tex, "src") ~= nil then
			props.src = rawget(tex, "src")
		elseif type(tex) == "number" then
			props.icon = tex
		end
		local w, h = U.vec(size)
		if w > 0 then
			props.width = math.max(2, math.floor(w / CELL + 0.5))
		end
		if h > 0 then
			props.height = math.max(1, math.floor(h / LINE + 0.5))
		end
		place(new_node("image", props))
	end

	function ImGui.PlotLines(lbl, values, count, offset, overlay, min, max, size)
		local vs = {}
		for _, v in ipairs(values or {}) do
			table.insert(vs, tonumber(v) or 0)
		end
		local w, h = U.vec(size)
		place(new_node("chart", {
			series = { { values = vs, label = clip(text_of(lbl)) } },
			min = num(min),
			max = num(max),
			width = w > 0 and math.floor(w) or 200,
			height = h > 0 and math.floor(h) or 60,
			style = "line",
		}))
	end
	function ImGui.PlotHistogram(lbl, values, count, offset, overlay, min, max, size)
		ImGui.PlotLines(lbl, values, count, offset, overlay, min, max, size)
		B.last.props.style = "bar"
	end

	-- Containers.
	local function push(node)
		place(node)
		table.insert(B.stack, node)
		return node
	end
	local function pop(kind)
		if #B.stack > 1 then
			table.remove(B.stack)
		end
	end
	function ImGui.BeginGroup()
		push(new_node("column", {}))
	end
	ImGui.EndGroup = pop
	function ImGui.BeginChild(id, size, border)
		local node = new_node(border and "group" or "column", {})
		local _, h = U.vec(size)
		if h > 0 then
			node.props.max_height = math.max(2, math.floor(h / LINE))
		end
		push(node)
		return true
	end
	ImGui.BeginChildFrame = ImGui.BeginChild
	ImGui.EndChild = pop
	ImGui.EndChildFrame = pop

	function ImGui.BeginTabBar(id)
		local key = key_of(id)
		local tabs = new_node("tabs", {}, key)
		tabs.pages = 0
		local p = pending(key)
		if p ~= nil and p.rows ~= nil and p.rows[1] ~= nil then
			state(B.hud).tab[key] = p.rows[1]
		end
		tabs.props.selected = state(B.hud).tab[key] or 1
		push(tabs)
		table.insert(B.ids, tostring(id))
		return true
	end
	function ImGui.EndTabBar()
		table.remove(B.ids)
		local tabs = container()
		if tabs.kind == "tabs" then
			if tabs.props.selected > #tabs.children then
				tabs.props.selected = math.max(1, #tabs.children)
			end
			pop()
		end
	end
	function ImGui.BeginTabItem(lbl)
		local tabs = container()
		if tabs.kind ~= "tabs" then
			return false
		end
		tabs.pages = tabs.pages + 1
		local page = new_node("group", { title = clip(text_of(lbl)) })
		table.insert(tabs.children, page)
		if tabs.props.selected == tabs.pages then
			table.insert(B.stack, page)
			table.insert(B.ids, tostring(lbl))
			return true
		end
		return false
	end
	function ImGui.EndTabItem()
		if container().kind == "group" then
			table.remove(B.ids)
			pop()
		end
	end

	-- Tree nodes and collapsing headers: a toggle that opens what follows.
	local function opener(lbl, default_open)
		local key = key_of(lbl)
		local open = state(B.hud).open[key]
		if open == nil then
			open = default_open == true
		end
		local p = pending(key)
		if p ~= nil and p.value ~= nil then
			open = p.value == true
		end
		state(B.hud).open[key] = open
		place(new_node("toggle", { text = clip(text_of(lbl)), value = open }, key))
		return open
	end
	function ImGui.CollapsingHeader(lbl, flagsv)
		if type(flagsv) == "boolean" then
			return opener(lbl, false), flagsv
		end
		return opener(lbl, has(flagsv, 32))
	end
	function ImGui.TreeNode(lbl, fmt)
		local open = opener(fmt ~= nil and (tostring(fmt) .. "##" .. tostring(lbl)) or lbl, false)
		if open then
			push(new_node("column", {}))
			table.insert(B.ids, tostring(lbl))
		end
		return open
	end
	function ImGui.TreeNodeEx(lbl, flagsv, fmt)
		local open = opener(fmt ~= nil and (tostring(fmt) .. "##" .. tostring(lbl)) or lbl, has(flagsv, 32))
		if open and not has(flagsv, 8) then
			push(new_node("column", {}))
			table.insert(B.ids, tostring(lbl))
		end
		return open
	end
	function ImGui.TreePush(id)
		push(new_node("column", {}))
		table.insert(B.ids, tostring(id))
	end
	function ImGui.TreePop()
		table.remove(B.ids)
		pop()
	end
	function ImGui.SetNextItemOpen(open)
		B.next_open = open
	end

	-- Tables: a row of cells for each table row; a table of nothing but text is a list.
	local function table_row(t)
		local row = new_node("row", {})
		row.cells = {}
		for i = 1, t.cols do
			local cell = new_node("column", { grow = 1 })
			row.cells[i] = cell
			table.insert(row.children, cell)
		end
		table.insert(t.node.children, row)
		t.row = row
		t.col = 0
		return row
	end
	function ImGui.BeginTable(id, cols, flagsv)
		local t = { cols = math.max(1, int_of(cols)), headers = {}, key = key_of(id), widths = {} }
		t.node = new_node("column", {})
		t.node.table = t
		place(t.node)
		table.insert(B.stack, t.node)
		B.tables = B.tables or {}
		table.insert(B.tables, t)
		table.insert(B.ids, tostring(id))
		return true
	end
	local function current_table()
		return B.tables and B.tables[#B.tables] or nil
	end
	function ImGui.TableSetupColumn(lbl, flagsv, width)
		local t = current_table()
		if t ~= nil then
			table.insert(t.headers, text_of(lbl))
			if has(flagsv, 16) and num(width) ~= nil and num(width) > 0 then
				t.widths[#t.headers] = math.max(3, math.floor(num(width) / CELL + 0.5))
			end
		end
	end
	function ImGui.TableSetupScrollFreeze() end
	function ImGui.TableHeadersRow()
		local t = current_table()
		if t == nil then
			return
		end
		t.has_headers = true
	end
	function ImGui.TableHeader(lbl)
		label(text_of(lbl))
	end
	local function enter_cell(t, i)
		if t.row == nil then
			table_row(t)
		end
		if i > t.cols then
			table_row(t)
			i = 1
		end
		t.col = i
		-- The stack holds the table, then the cell items go to.
		while container() ~= t.node and #B.stack > 1 do
			table.remove(B.stack)
		end
		table.insert(B.stack, t.row.cells[i])
		B.sameline = false
	end
	function ImGui.TableNextRow()
		local t = current_table()
		if t == nil then
			return
		end
		while container() ~= t.node and #B.stack > 1 do
			table.remove(B.stack)
		end
		table_row(t)
	end
	function ImGui.TableNextColumn()
		local t = current_table()
		if t == nil then
			return false
		end
		enter_cell(t, (t.col or 0) + 1)
		return true
	end
	function ImGui.TableSetColumnIndex(i)
		local t = current_table()
		if t == nil then
			return false
		end
		enter_cell(t, int_of(i) + 1)
		return true
	end
	function ImGui.TableGetColumnIndex()
		local t = current_table()
		return t and math.max(0, (t.col or 1) - 1) or 0
	end
	function ImGui.TableGetRowIndex()
		local t = current_table()
		return t and math.max(0, #t.node.children - 1) or 0
	end
	function ImGui.TableGetColumnCount()
		local t = current_table()
		return t and t.cols or 0
	end
	function ImGui.TableGetSortSpecs()
		return nil
	end
	function ImGui.TableSetBgColor() end
	function ImGui.EndTable()
		local t = current_table()
		if t == nil then
			return
		end
		table.remove(B.tables)
		table.remove(B.ids)
		while container() ~= t.node and #B.stack > 1 do
			table.remove(B.stack)
		end
		if container() == t.node then
			table.remove(B.stack)
		end
		-- A table whose cells are each one line of text, or nothing, is shown as a list.
		local plain = true
		local rows = {}
		for _, row in ipairs(t.node.children) do
			local cells = {}
			for i, cell in ipairs(row.cells or {}) do
				local kids = cell.children
				if #kids == 0 then
					cells[i] = ""
				elseif #kids == 1 and kids[1].kind == "label" then
					local c = kids[1].props.color
					cells[i] = c and { text = kids[1].props.text, color = c } or kids[1].props.text
				else
					plain = false
				end
			end
			table.insert(rows, cells)
		end
		if plain and #rows > 0 then
			local columns = {}
			for i = 1, t.cols do
				local h = t.headers[i] or ""
				if t.widths[i] ~= nil then
					columns[i] = { title = h, width = t.widths[i] }
				else
					columns[i] = h
				end
			end
			t.node.kind = "list"
			t.node.props = { columns = columns, rows = rows, height = math.min(#rows, 20) }
			t.node.children = {}
		elseif t.has_headers then
			local head = new_node("row", {})
			for i = 1, t.cols do
				table.insert(head.children, new_node("label", { text = clip(t.headers[i] or ""), grow = 1 }))
			end
			table.insert(t.node.children, 1, head)
		end
	end
	-- The old columns: `Columns(n)` and `NextColumn()`, as a table.
	function ImGui.Columns(count, id)
		local n = int_of(count or 1)
		local t = current_table()
		if t ~= nil and t.columns_api then
			ImGui.EndTable()
		end
		if n > 1 then
			ImGui.BeginTable(id or "columns", n)
			current_table().columns_api = true
			ImGui.TableNextColumn()
		end
	end
	function ImGui.NextColumn()
		ImGui.TableNextColumn()
	end
	function ImGui.GetColumnIndex()
		return ImGui.TableGetColumnIndex()
	end
	function ImGui.GetColumnsCount()
		return ImGui.TableGetColumnCount()
	end
	function ImGui.SetColumnWidth() end
	function ImGui.GetColumnWidth()
		return 100
	end

	-- Popups and menus: a popup a script opened is shown in the window as a group, until closed.
	function ImGui.OpenPopup(id)
		state(B.hud).popups[tostring(id)] = true
	end
	local function begin_popup(id)
		if not state(B.hud).popups[tostring(id)] then
			return false
		end
		push(new_node("group", { title = clip(text_of(id)) }))
		B.popup = B.popup or {}
		table.insert(B.popup, tostring(id))
		return true
	end
	function ImGui.BeginPopup(id)
		return begin_popup(id)
	end
	function ImGui.BeginPopupModal(id, open)
		local shown = begin_popup(id)
		if open ~= nil then
			return shown, open
		end
		return shown
	end
	function ImGui.EndPopup()
		if B.popup ~= nil and #B.popup > 0 then
			table.remove(B.popup)
			pop()
		end
	end
	function ImGui.CloseCurrentPopup()
		if B.popup ~= nil and #B.popup > 0 then
			state(B.hud).popups[B.popup[#B.popup]] = nil
		end
	end
	function ImGui.IsPopupOpen(id)
		return state(B.hud).popups[tostring(id)] == true
	end
	function ImGui.BeginPopupContextItem()
		return false
	end
	ImGui.BeginPopupContextWindow = ImGui.BeginPopupContextItem
	ImGui.BeginPopupContextVoid = ImGui.BeginPopupContextItem
	ImGui.OpenPopupOnItemClick = function() end
	function ImGui.BeginMenuBar()
		push(new_node("row", {}))
		return true
	end
	ImGui.EndMenuBar = pop
	function ImGui.BeginMainMenuBar()
		return false
	end
	function ImGui.EndMainMenuBar() end
	function ImGui.BeginMenu(lbl)
		return opener(lbl, false) and (function()
			push(new_node("column", {}))
			return true
		end)()
	end
	function ImGui.EndMenu()
		pop()
	end

	-- Tooltips: what an item shows when the pointer rests on it.
	function ImGui.SetTooltip(text, ...)
		if B.last ~= nil then
			B.last.props.tooltip = clip(text)
		end
	end
	ImGui.SetItemTooltip = ImGui.SetTooltip
	function ImGui.BeginTooltip()
		B.tooltip = {}
		return true
	end
	ImGui.BeginItemTooltip = ImGui.BeginTooltip
	function ImGui.EndTooltip()
		local lines = B.tooltip
		B.tooltip = nil
		if lines ~= nil and B.last ~= nil and #lines > 0 then
			B.last.props.tooltip = clip(table.concat(lines, "\n"))
		end
	end
	-- The window draws a tooltip for each item that has one, so an item is taken to be hovered
	-- for what it shows; nothing else hovers or is pressed here.
	function ImGui.IsItemHovered()
		return B.last ~= nil
	end
	function ImGui.IsItemClicked()
		return B.last_clicked == true
	end
	for _, n in ipairs({
		"IsItemActive", "IsItemActivated", "IsItemDeactivated", "IsItemDeactivatedAfterEdit",
		"IsItemEdited", "IsItemFocused", "IsItemToggledOpen", "IsAnyItemActive", "IsAnyItemFocused",
		"IsAnyItemHovered", "IsAnyMouseDown", "IsMouseClicked", "IsMouseDoubleClicked", "IsMouseDown",
		"IsMouseDragging", "IsMouseReleased", "IsMouseHoveringRect", "IsKeyDown", "IsKeyPressed",
		"IsKeyReleased", "IsWindowAppearing", "IsWindowCollapsed", "IsWindowDocked", "IsWindowHovered",
	}) do
		ImGui[n] = function()
			return false
		end
	end
	function ImGui.IsItemVisible()
		return true
	end
	function ImGui.IsWindowFocused()
		return false
	end
	function ImGui.IsRectVisible()
		return true
	end

	-- Style and state that a declared window lays out for itself.
	for _, n in ipairs({
		"PushStyleVar", "PopStyleVar", "PushItemWidth", "PopItemWidth", "SetNextItemWidth",
		"PushTextWrapPos", "PopTextWrapPos", "PushFont", "PopFont", "PushButtonRepeat",
		"PopButtonRepeat", "PushTabStop", "PopTabStop", "SetItemDefaultFocus", "SetKeyboardFocusHere",
		"SetNextWindowBgAlpha", "SetNextWindowFocus", "SetWindowFocus", "SetWindowFontScale",
		"SetScrollHereY", "SetScrollHereX", "SetScrollY", "SetScrollX", "SetScrollFromPosY",
		"SetScrollFromPosX", "SetCursorPos", "SetCursorPosX", "SetCursorPosY", "SetCursorScreenPos",
		"SetNextWindowCollapsed", "SetWindowCollapsed", "SetNextWindowSizeConstraints",
		"SetNextWindowContentSize", "SetNextWindowScroll", "SetNextItemAllowOverlap",
		"SetMouseCursor", "SetNextFrameWantCaptureMouse", "SetNextFrameWantCaptureKeyboard",
		"StyleColorsDark", "StyleColorsLight", "StyleColorsClassic", "PushClipRect", "PopClipRect",
		"SetColumnOffset", "LogText", "LogFinish", "LogToClipboard", "LogButtons",
	}) do
		ImGui[n] = function() end
	end
	function ImGui.PushStyleColor(idx, color)
		B.colors = B.colors or {}
		table.insert(B.colors, { idx = num(idx), prev = B.color })
		if num(idx) == 0 then
			B.color = color_of(color)
		end
	end
	function ImGui.PopStyleColor(n)
		for _ = 1, int_of(n or 1) do
			local c = B.colors and table.remove(B.colors)
			if c ~= nil and c.idx == 0 then
				B.color = c.prev
			end
		end
	end
	function ImGui.BeginDisabled(disabled)
		if disabled == nil or disabled == true then
			B.disabled = B.disabled + 1
		else
			B.disabled_noop = (B.disabled_noop or 0) + 1
		end
	end
	function ImGui.EndDisabled()
		if (B.disabled_noop or 0) > 0 then
			B.disabled_noop = B.disabled_noop - 1
		elseif B.disabled > 0 then
			B.disabled = B.disabled - 1
		end
	end
	function ImGui.PushID(id)
		table.insert(B.ids, tostring(id))
	end
	function ImGui.PopID()
		table.remove(B.ids)
	end
	function ImGui.GetID(id)
		return string.len(table.concat(B.ids, "/") .. tostring(id))
	end

	-- Measures.
	local function window_px(h)
		local s = state(h)
		return (s.width or 40) * CELL, (s.height or 20) * LINE
	end
	function ImGui.SetWindowSize(size, cond)
		local w, h = U.vec(size)
		local s = state(B.hud)
		if s.sized and num(cond) == 4 then
			return
		end
		s.sized = true
		if w > 0 then
			s.width = math.max(10, math.floor(w / CELL + 0.5))
		end
		if h > 0 then
			s.height = math.max(3, math.floor(h / LINE + 0.5))
		end
	end
	ImGui.SetNextWindowSize = function(size, cond)
		if B ~= nil then
			ImGui.SetWindowSize(size, cond)
		end
	end
	function ImGui.SetWindowPos() end
	function ImGui.SetNextWindowPos() end
	function ImGui.GetWindowPos()
		return V2(0, 0)
	end
	function ImGui.GetWindowSize()
		return V2(window_px(B.hud))
	end
	function ImGui.GetWindowWidth()
		return (window_px(B.hud))
	end
	function ImGui.GetWindowHeight()
		local _, h = window_px(B.hud)
		return h
	end
	function ImGui.GetContentRegionAvail()
		local w, h = window_px(B.hud)
		return V2(w - 16, h - 16)
	end
	ImGui.GetContentRegionMax = ImGui.GetContentRegionAvail
	ImGui.GetWindowContentRegionMax = ImGui.GetContentRegionAvail
	function ImGui.GetWindowContentRegionMin()
		return V2(0, 0)
	end
	function ImGui.GetCursorPos()
		return V2(0, 0)
	end
	ImGui.GetCursorScreenPos = ImGui.GetCursorPos
	ImGui.GetCursorStartPos = ImGui.GetCursorPos
	function ImGui.GetCursorPosX()
		return 0
	end
	ImGui.GetCursorPosY = ImGui.GetCursorPosX
	function ImGui.CalcTextSize(text)
		return V2(#tostring(text or "") * CELL, LINE)
	end
	function ImGui.GetTextLineHeight()
		return LINE
	end
	function ImGui.GetTextLineHeightWithSpacing()
		return LINE + 3
	end
	function ImGui.GetFrameHeight()
		return 22
	end
	function ImGui.GetFrameHeightWithSpacing()
		return 25
	end
	function ImGui.GetFontSize()
		return 13
	end
	function ImGui.CalcItemWidth()
		return 140
	end
	function ImGui.GetItemRectSize()
		return V2(100, 22)
	end
	ImGui.GetItemRectMin = ImGui.GetCursorPos
	ImGui.GetItemRectMax = ImGui.GetItemRectSize
	function ImGui.GetScrollY()
		return 0
	end
	ImGui.GetScrollX = ImGui.GetScrollY
	ImGui.GetScrollMaxY = ImGui.GetScrollY
	ImGui.GetScrollMaxX = ImGui.GetScrollY
	function ImGui.GetTime()
		return D.os.clock()
	end
	local frames = 0
	function ImGui.GetFrameCount()
		return frames
	end
	function ImGui.GetMousePos()
		return V2(-1, -1)
	end
	function ImGui.GetMouseDragDelta()
		return V2(0, 0)
	end
	function ImGui.GetClipboardText()
		return ""
	end
	function ImGui.SetClipboardText()
		error(U.refusal("ImGui.SetClipboardText", "a plugin does not reach the clipboard"), 2)
	end
	local IO = U.class("ImGuiIO")
	IO.define({
		get = {
			DisplaySize = function()
				return V2(1920, 1080)
			end,
			DeltaTime = function()
				return 0.1
			end,
			Framerate = function()
				return 10
			end,
			WantCaptureMouse = function()
				return false
			end,
			WantCaptureKeyboard = function()
				return false
			end,
			WantTextInput = function()
				return false
			end,
			KeyCtrl = function()
				return false
			end,
			KeyShift = function()
				return false
			end,
			KeyAlt = function()
				return false
			end,
			MousePos = function()
				return V2(-1, -1)
			end,
			FontGlobalScale = function()
				return 1
			end,
		},
	})
	function ImGui.GetIO()
		return IO.new({})
	end
	local Style = U.class("ImGuiStyle")
	Style.fallback = function()
		return true, V2(4, 4)
	end
	function ImGui.GetStyle()
		return Style.new({})
	end
	function ImGui.GetColorU32(c)
		return 0xFFFFFFFF
	end
	function ImGui.ColorConvertFloat4ToU32(c)
		local x, y, z, w = U.vec(c)
		local function b(f)
			return math.max(0, math.min(255, math.floor(f * 255 + 0.5)))
		end
		return b(x) + b(y) * 256 + b(z) * 65536 + b(w) * 16777216
	end
	function ImGui.ColorConvertU32ToFloat4(u)
		u = num(u) or 0
		return U.Vector4(
			bit32.band(u, 0xFF) / 255,
			bit32.band(bit32.rshift(u, 8), 0xFF) / 255,
			bit32.band(bit32.rshift(u, 16), 0xFF) / 255,
			bit32.band(bit32.rshift(u, 24), 0xFF) / 255
		)
	end
	ImGui.ColToVec4 = ImGui.ColorConvertU32ToFloat4
	ImGui.Vec4ToCol = ImGui.ColorConvertFloat4ToU32
	function ImGui.ColorEdit3(lbl, color)
		label(text_of(lbl), { color = color_of(color) })
		return false, color
	end
	ImGui.ColorEdit4 = ImGui.ColorEdit3
	ImGui.ColorPicker3 = ImGui.ColorEdit3
	ImGui.ColorPicker4 = ImGui.ColorEdit3
	function ImGui.ColorButton(id, color)
		label(text_of(id), { color = color_of(color) })
		return false
	end

	-- Windows of the script's own: `Begin(name)`, its items, `End()`. Each is a window of its own,
	-- shown while the script begins it on each render, and closed once it does not.
	local own_windows = {}
	function ImGui.Begin(name, open)
		local key = tostring(name)
		local hud = own_windows[key]
		if hud == nil then
			hud = U.hud(key, nil, "window")
			own_windows[key] = hud
		end
		local hs = state(hud)
		hs.begun = true
		if not hs.visible and not hs.closed_by_player then
			hs.visible = true
			hs.signature = nil
		end
		B = U.begin_render(hud)
		if type(open) == "boolean" then
			return true, not hs.closed_by_player
		end
		return true
	end
	function ImGui.End()
		local b = B
		if b == nil or b.outer_of == nil then
			return
		end
		B = b.outer_of
		U.end_render(b)
	end

	local refused_draw = "a window is declared, not drawn at pixels; draw on a canvas node of a plugin window"
	for _, n in ipairs({ "GetWindowDrawList", "GetBackgroundDrawList", "GetForegroundDrawList" }) do
		ImGui[n] = function()
			error(U.refusal("ImGui." .. n, refused_draw), 2)
		end
	end
	for _, n in ipairs({
		"ShowDemoWindow", "ShowMetricsWindow", "ShowAboutWindow", "ShowStyleEditor", "ShowUserGuide",
		"ShowDebugLogWindow", "ShowStackToolWindow", "ShowFontSelector", "ShowStyleSelector",
		"BeginDragDropSource", "BeginDragDropTarget", "AcceptDragDropPayload", "SetDragDropPayload",
		"DockSpace", "DockSpaceOverViewport", "CreateContext", "DestroyContext", "NewFrame", "Render",
		"EndFrame",
	}) do
		ImGui[n] = function()
			error(U.refusal("ImGui." .. n, "a plugin's windows are declared to the host, which draws them"), 2)
		end
	end
	ImGui.EndDragDropSource = function() end
	ImGui.EndDragDropTarget = function() end

	-- Inputs of two, three or four numbers: a row of number nodes, the value a vector (or a list).
	local function multi(kind, count, integer)
		return function(lbl, v, a, b)
			local vals = {}
			local is_vector = type(v) == "table" and rawget(v, U.S) ~= nil
			if is_vector then
				local x, y, z, w = U.vec(v)
				vals = { x, y, z, w }
			elseif type(v) == "table" then
				for i = 1, count do
					vals[i] = tonumber(v[i]) or 0
				end
			end
			local row = new_node("row", {})
			place(row)
			table.insert(B.stack, row)
			table.insert(B.ids, tostring(lbl))
			local changed = false
			local out = {}
			for i = 1, count do
				local name = (i == 1 and text_of(lbl) or "") .. "##" .. i
				local c, nv
				if kind == "slider" then
					local lo, hi = num(a) or 0, num(b) or (integer and 100 or 1)
					c, nv = input("slider", name, vals[i] or 0, { min = lo, max = hi, step = integer and 1 or (hi - lo) / 100 }, integer and int_of or tonumber)
				else
					c, nv = input("number", name, vals[i] or 0, { step = integer and 1 or 0.1 }, integer and int_of or tonumber)
				end
				changed = changed or c
				out[i] = nv
			end
			table.remove(B.ids)
			pop()
			if not changed then
				return false, v
			end
			if is_vector then
				if count == 2 then
					return true, U.Vector2(out[1], out[2])
				elseif count == 3 then
					return true, U.Vector3(out[1], out[2], out[3])
				end
				return true, U.Vector4(out[1], out[2], out[3], out[4])
			end
			return true, out
		end
	end
	for n = 2, 4 do
		ImGui["InputFloat" .. n] = multi("number", n, false)
		ImGui["InputInt" .. n] = multi("number", n, true)
		ImGui["DragFloat" .. n] = multi("number", n, false)
		ImGui["DragInt" .. n] = multi("number", n, true)
		ImGui["SliderFloat" .. n] = multi("slider", n, false)
		ImGui["SliderInt" .. n] = multi("slider", n, true)
	end
	local function range(integer)
		return function(lbl, lo, hi)
			local c1, a = input("number", text_of(lbl) .. "##min", lo, { step = integer and 1 or 0.1 }, integer and int_of or tonumber)
			B.sameline = true
			local c2, b = input("number", "##max" .. tostring(lbl), hi, { step = integer and 1 or 0.1 }, integer and int_of or tonumber)
			return c1 or c2, a, b
		end
	end
	ImGui.DragFloatRange2 = range(false)
	ImGui.DragIntRange2 = range(true)
	function ImGui.VSliderFloat(lbl, size, v, min, max)
		return ImGui.SliderFloat(lbl, v, min, max)
	end
	function ImGui.VSliderInt(lbl, size, v, min, max)
		return ImGui.SliderInt(lbl, v, min, max)
	end
	function ImGui.Value(prefix, v)
		label(tostring(prefix) .. ": " .. tostring(v))
	end
	function ImGui.TextureButton(id)
		return button(tostring(id))
	end
	function ImGui.ColorConvertHSVtoRGB(h, s, v)
		h, s, v = tonumber(h) or 0, tonumber(s) or 0, tonumber(v) or 0
		if s == 0 then
			return v, v, v
		end
		h = (h % 1) * 6
		local i = math.floor(h)
		local f = h - i
		local p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
		local rgb = { { v, t, p }, { q, v, p }, { p, v, t }, { p, q, v }, { t, p, v }, { v, p, q } }
		local c = rgb[i + 1]
		return c[1], c[2], c[3]
	end
	function ImGui.ColorConvertRGBtoHSV(r, g, b)
		r, g, b = tonumber(r) or 0, tonumber(g) or 0, tonumber(b) or 0
		local mx, mn = math.max(r, g, b), math.min(r, g, b)
		local d = mx - mn
		local h = 0
		if d > 0 then
			if mx == r then
				h = ((g - b) / d) % 6
			elseif mx == g then
				h = (b - r) / d + 2
			else
				h = (r - g) / d + 4
			end
			h = h / 6
		end
		return h, mx == 0 and 0 or d / mx, mx
	end
	function ImGui.GetStyleColorVec4()
		return U.Vector4(1, 1, 1, 1)
	end
	function ImGui.GetStyleColorName(idx)
		for k, v in pairs(FLAGS.ImGuiCol) do
			if v == num(idx) then
				return k
			end
		end
		return ""
	end
	function ImGui.GetVersion()
		return "1.89"
	end
	local Viewport = U.class("ImGuiViewport")
	Viewport.define({
		get = {
			Pos = function()
				return V2(0, 0)
			end,
			Size = function()
				return V2(1920, 1080)
			end,
			WorkPos = function()
				return V2(0, 0)
			end,
			WorkSize = function()
				return V2(1920, 1080)
			end,
		},
	})
	function ImGui.GetMainViewport()
		return Viewport.new({})
	end
	function ImGui.GetWindowDpiScale()
		return 1
	end
	function ImGui.GetMouseCursor()
		return 0
	end
	function ImGui.GetMouseClickedCount()
		return 0
	end
	function ImGui.GetMousePosOnOpeningCurrentPopup()
		return V2(-1, -1)
	end
	function ImGui.GetKeyName(k)
		return tostring(k)
	end
	function ImGui.GetKeyIndex(k)
		return k
	end
	function ImGui.GetKeyPressedAmount()
		return 0
	end
	function ImGui.IsMousePosValid()
		return false
	end
	function ImGui.ResetMouseDragDelta() end
	function ImGui.GetTreeNodeToLabelSpacing()
		return 16
	end
	function ImGui.GetColumnOffset()
		return 0
	end
	function ImGui.GetItemID()
		return 0
	end
	function ImGui.TableGetColumnName(i)
		local t = current_table()
		return t and t.headers[(num(i) or 0) + 1] or ""
	end
	function ImGui.TableGetColumnFlags()
		return 0
	end
	function ImGui.TableSetColumnEnabled() end
	function ImGui.SetTabItemClosed() end
	function ImGui.LogToFile() end
	function ImGui.LogToTTY() end
	function ImGui.SetColorEditOptions() end
	local host_draws = "a plugin's windows are declared to the host, which draws them"
	for _, n in ipairs({
		"DragScalar", "DragScalarN", "InputScalar", "InputScalarN", "SliderScalar", "SliderScalarN",
		"VSliderScalar", "DebugCheckVersionAndDataLayout", "DebugTextEncoding", "DestroyPlatformWindows",
		"FindViewportByID", "FindViewportByPlatformHandle", "GetAllocatorFunctions", "GetCurrentContext",
		"GetDragDropPayload", "GetDrawData", "GetDrawListSharedData", "GetFont", "GetFontTexUvWhitePixel",
		"GetPlatformIO", "GetStateStorage", "GetWindowDockID", "GetWindowViewport",
		"LoadIniSettingsFromDisk", "LoadIniSettingsFromMemory", "SaveIniSettingsToDisk",
		"SaveIniSettingsToMemory", "MemAlloc", "MemFree", "RenderPlatformWindowsDefault",
		"SetAllocatorFunctions", "SetCurrentContext", "SetNextWindowClass", "SetNextWindowDockID",
		"SetNextWindowViewport", "SetStateStorage", "UpdatePlatformWindows",
	}) do
		ImGui[n] = function()
			error(U.refusal("ImGui." .. n, host_draws), 2)
		end
	end

	local ImGuiTable = setmetatable({}, {
		__index = function(_, key)
			if type(key) ~= "string" then
				return nil
			end
			local f = rawget(ImGui, key)
			if f == nil then
				local n = norm(key)
				for k, v in pairs(ImGui) do
					if norm(k) == n then
						f = v
						break
					end
				end
			end
			if f == nil then
				error("ImGui has no function " .. key, 2)
			end
			if B == nil then
				return function()
					error("ImGui." .. key .. " can only be called while a HUD renders", 2)
				end
			end
			return f
		end,
		__newindex = function()
			error("ImGui is read-only", 2)
		end,
	})

	-- HUDs ------------------------------------------------------------------------------------------

	local Hud = class("Hud")
	U.Hud = Hud
	local huds = {} -- every HUD and window of the script's, by window id
	local next_window = 0
	local MAX_WINDOWS = 30

	local function window_id_for(name)
		next_window = next_window + 1
		local base = string.lower((string.gsub(tostring(name), "[^%w]+", "-")))
		base = string.sub((string.gsub(base, "^%-+", "")), 1, 40)
		if base == "" then
			base = "hud"
		end
		return "ub" .. next_window .. "-" .. base
	end

	local function title_of(s)
		return string.sub(tostring(s.title or s.name), 1, 200)
	end

	-- The window's declaration: its id, title and size, and the nodes a render built.
	local function window_table(h, nodes)
		local s = state(h)
		local t = { id = s.window, title = title_of(s) }
		if s.width ~= nil then
			t.width = s.width
		end
		if s.height ~= nil then
			t.height = s.height
		end
		if has(s.flags, 2) or has(s.flags, 64) then
			t.resizable = false
		end
		for _, n in ipairs(nodes) do
			table.insert(t, n)
		end
		return t
	end

	local function out_node(n)
		local t = { kind = n.kind, id = n.id, children = {} }
		for k, v in pairs(n.props) do
			-- A selectable's own mark is the list's to show, not a label's.
			if k ~= "selected" or n.kind == "list" or n.kind == "tabs" then
				t[k] = v
			end
		end
		for _, c in ipairs(n.children) do
			table.insert(t.children, out_node(c))
		end
		return t
	end

	-- The shape of a tree (its nodes' kinds, ids and nesting), and each node's properties.
	local function shape(n, out, props)
		table.insert(out, n.kind .. "#" .. n.id .. "(")
		props[n.id] = n.props
		for _, c in ipairs(n.children) do
			shape(c, out, props)
		end
		table.insert(out, ")")
	end

	local function same(a, b)
		if type(a) ~= type(b) then
			return false
		end
		if type(a) ~= "table" then
			return a == b
		end
		for k, v in pairs(a) do
			if not same(v, b[k]) then
				return false
			end
		end
		for k in pairs(b) do
			if a[k] == nil then
				return false
			end
		end
		return true
	end

	-- Declare the window afresh with these nodes; a tree the host refuses shows why instead.
	local function declare(h, nodes)
		local s = state(h)
		local ok, w = pcall(D.ui.window, window_table(h, nodes))
		if not ok then
			D.print("the window " .. s.name .. " could not be shown: " .. tostring(w))
			ok, w = pcall(D.ui.window, {
				id = s.window,
				title = title_of(s),
				D.ui.label({ text = "This window could not be shown: " .. string.sub(tostring(w), 1, 300) }),
			})
		end
		if ok then
			s.handle = w
		end
	end

	-- Bring the window to what a render built: the properties that changed, or the whole tree when
	-- its shape changed (or too much of it to set node by node).
	local function reconcile(h, root)
		local s = state(h)
		local parts, props = {}, {}
		for _, c in ipairs(root.children) do
			shape(c, parts, props)
		end
		local sig = table.concat(parts) .. "|" .. tostring(s.width) .. "x" .. tostring(s.height) .. "|" .. title_of(s)
		if sig == s.signature then
			local changes = {}
			for id, p in pairs(props) do
				local was = s.props[id] or {}
				for k, v in pairs(p) do
					if not same(v, was[k]) then
						table.insert(changes, { id, k, v })
					end
				end
				for k in pairs(was) do
					if p[k] == nil then
						table.insert(changes, { id, k, nil })
					end
				end
			end
			if #changes <= 24 then
				for _, c in ipairs(changes) do
					local ok = pcall(function()
						D.ui.get(s.window .. "." .. c[1])[c[2]] = c[3]
					end)
					if not ok then
						s.signature = nil
					end
				end
				s.props = props
				if s.signature ~= nil then
					return
				end
			end
		end
		local out = {}
		for _, c in ipairs(root.children) do
			table.insert(out, out_node(c))
		end
		declare(h, out)
		s.signature = sig
		s.props = props
	end

	function U.begin_render(h)
		local root = { kind = "column", props = {}, children = {}, id = "root" }
		return {
			hud = h,
			root = root,
			stack = { root },
			ids = {},
			seen = {},
			seq = 0,
			keys = {},
			disabled = 0,
			outer_of = B,
		}
	end

	function U.end_render(b)
		local s = state(b.hud)
		s.keys = b.keys
		if s.visible then
			reconcile(b.hud, b.root)
		end
	end

	-- Render a HUD: its handlers, then its window brought to what they built. A HUD that draws its
	-- own windows renders whether its default window shows or not; what it builds outside its
	-- `Begin` and `End` goes nowhere.
	local function render(h)
		local s = state(h)
		if s.kind == "window" or (not s.visible and not s.own) then
			return
		end
		frames = frames + 1
		U.fire(s.events.OnPreRender, nil)
		local prev = B
		local b = U.begin_render(h)
		B = b
		for _, hd in ipairs(table.clone(state(s.events.OnRender).handlers)) do
			local ok, err = pcall(hd.fn)
			if not ok then
				U.note_error(err)
			end
			-- What a handler left open (a `Begin` with no `End`) is closed for it.
			while B ~= b and B ~= nil and B.outer_of ~= nil do
				local inner = B
				B = inner.outer_of
				U.end_render(inner)
			end
			B = b
		end
		B = prev
		if not s.own then
			U.end_render(b)
		end
		U.fire(s.events.OnPostRender, nil)
	end
	U.render = render

	local function render_all()
		for _, h in pairs(huds) do
			state(h).begun = false
		end
		for _, h in pairs(huds) do
			render(h)
		end
		-- A window of the script's own that no render began is closed.
		for _, h in pairs(huds) do
			local s = state(h)
			if s.kind == "window" and s.visible and not s.begun then
				s.visible = false
				if s.handle ~= nil then
					pcall(s.handle.close, s.handle)
				end
			end
		end
	end

	local function set_visible(h, v)
		local s = state(h)
		if s.visible == v then
			return
		end
		s.visible = v
		if v then
			s.closed_by_player = false
			s.signature = nil
			U.fire(s.events.OnShow, nil)
			render(h)
			if s.handle ~= nil then
				pcall(s.handle.show, s.handle)
			end
		else
			U.fire(s.events.OnHide, nil)
			if s.handle ~= nil then
				pcall(s.handle.close, s.handle)
			end
		end
	end

	-- A HUD of the script's (`kind` "window" for a window it begins itself): its window, declared
	-- now and closed so that the plugin bar lists it, its events, and what the player did on it.
	function U.hud(name, icon, kind)
		local count = 0
		for _ in pairs(huds) do
			count = count + 1
		end
		if count >= MAX_WINDOWS then
			error("a script may have at most " .. MAX_WINDOWS .. " HUDs and windows", 3)
		end
		local s = {
			name = tostring(name),
			icon = icon,
			kind = kind or "hud",
			visible = false,
			show_in_bar = false,
			flags = 0,
			window = window_id_for(name),
			pending = {},
			tab = {},
			open = {},
			chosen = {},
			popups = {},
			keys = {},
			props = {},
			events = {},
		}
		local h = Hud.new(s)
		for _, n in ipairs({ "OnRender", "OnPreRender", "OnPostRender", "OnShow", "OnHide", "OnCreateTextures", "OnDestroyTextures" }) do
			s.events[n] = U.event(n)
		end
		huds[s.window] = h
		U.ensure_ui()
		declare(h, {})
		if s.handle ~= nil then
			pcall(s.handle.close, s.handle)
		end
		return h
	end

	Hud.define({
		get = {
			Name = function(h)
				return state(h).name
			end,
			Title = function(h)
				return title_of(state(h))
			end,
			Visible = function(h)
				return state(h).visible
			end,
			ShowInBar = function(h)
				return state(h).show_in_bar
			end,
			WindowSettings = function(h)
				return state(h).flags
			end,
			DontDrawDefaultWindow = function(h)
				return state(h).own == true
			end,
			OnRender = function(h)
				return state(h).events.OnRender
			end,
			OnPreRender = function(h)
				return state(h).events.OnPreRender
			end,
			OnPostRender = function(h)
				return state(h).events.OnPostRender
			end,
			OnShow = function(h)
				return state(h).events.OnShow
			end,
			OnHide = function(h)
				return state(h).events.OnHide
			end,
			OnCreateTextures = function(h)
				return state(h).events.OnCreateTextures
			end,
			OnDestroyTextures = function(h)
				return state(h).events.OnDestroyTextures
			end,
			WindowX = function()
				return 0
			end,
			WindowY = function()
				return 0
			end,
			LastRenderTimeMS = function()
				return 0
			end,
			LastPreRenderTimeMS = function()
				return 0
			end,
			LastPostRenderTimeMS = function()
				return 0
			end,
		},
		set = {
			Visible = function(h, v)
				set_visible(h, v == true)
			end,
			ShowInBar = function(h, v)
				state(h).show_in_bar = v == true
			end,
			WindowSettings = function(h, v)
				state(h).flags = num(v) or 0
				state(h).signature = nil
			end,
			Title = function(h, v)
				state(h).title = v
				state(h).signature = nil
			end,
			DontDrawDefaultWindow = function(h, v)
				state(h).own = v == true
			end,
			WindowX = function() end,
			WindowY = function() end,
		},
		call = {
			Dispose = function(h)
				local s = state(h)
				set_visible(h, false)
				huds[s.window] = nil
			end,
		},
	})
	Hud.tostring = function(h)
		return "Hud " .. state(h).name
	end

	-- The player's acts on a HUD's window, kept for its next render, which follows at once.
	local ui_listening = false
	function U.ensure_ui()
		if ui_listening then
			return
		end
		ui_listening = true
		local function acted(e, what)
			local h = huds[e.window]
			if h == nil then
				return
			end
			local s = state(h)
			local key = e.id and s.keys[e.id] or nil
			if key ~= nil then
				s.pending[key] = what
			end
			if s.kind == "window" then
				-- A window of the script's own renders with the HUD that begins it.
				for _, o in pairs(huds) do
					if state(o).kind ~= "window" then
						render(o)
					end
				end
			else
				render(h)
			end
		end
		U.listen("ui.clicked", function(e)
			acted(e, { clicked = true })
		end)
		U.listen("ui.changed", function(e)
			acted(e, { value = e.value })
		end)
		U.listen("ui.selected", function(e)
			acted(e, { rows = e.rows })
		end)
		U.listen("ui.closed", function(e)
			local h = huds[e.window]
			if h ~= nil then
				local s = state(h)
				s.visible = false
				s.closed_by_player = true
				U.fire(s.events.OnHide, nil)
			end
		end)
		U.listen("ui.opened", function(e)
			local h = huds[e.window]
			if h ~= nil then
				local s = state(h)
				s.closed_by_player = false
				if not s.visible then
					s.visible = true
					s.signature = nil
					U.fire(s.events.OnShow, nil)
					render(h)
				end
			end
		end)
		U.every(100, render_all)
		U.at_end(function()
			for _, h in pairs(huds) do
				local s = state(h)
				if s.handle ~= nil then
					pcall(s.handle.close, s.handle)
				end
			end
			huds = {}
			own_windows = {}
			ui_listening = false
		end)
	end

	-- Textures: an icon from the game's data, or a picture in the script's folder.
	local Texture = class("ManagedTexture")
	Texture.define({
		get = {
			TexturePtr = function(t)
				return state(t).ptr
			end,
			Width = function()
				return 32
			end,
			Height = function()
				return 32
			end,
		},
		call = {
			Dispose = function() end,
		},
	})
	local function texture(ptr)
		return Texture.new({ ptr = ptr })
	end

	local Toaster = class("Toaster")
	Toaster.define({
		call = {
			Add = function(_, text)
				pcall(D.ui.toast, tostring(text))
			end,
			Init = function() end,
			Dispose = function() end,
		},
	})

	local HudManager = class("ScriptHudManager")
	HudManager.define({
		get = {
			Toaster = function()
				return Toaster.new({})
			end,
		},
		call = {
			CreateHud = function(_, name, icon)
				return U.hud(name, icon, false)
			end,
			GetIconTexture = function(_, icon)
				return texture({ icon = num(icon) })
			end,
			CreateTexture = function(_, file)
				return texture({ src = tostring(file) })
			end,
			Dispose = function() end,
		},
	})

	U.modules["utilitybelt.views"] = function()
		return { Huds = HudManager.new({}) }
	end
	U.modules["imgui"] = function()
		local m = { ImGui = ImGuiTable }
		for name in pairs(FLAGS) do
			m[name] = flags(name)
		end
		return setmetatable(m, {
			__index = function(_, k)
				-- Every other of ImGui's flag and enumeration tables reads 0 for each name.
				if type(k) == "string" and string.sub(k, 1, 5) == "ImGui" then
					return flags(k)
				end
				return nil
			end,
		})
	end
	U.modules["implot"] = function()
		error(U.refusal("implot", "plots are drawn on a chart node of a plugin window"), 2)
	end

	function U.note_error(err)
		D.print(tostring(err))
	end
end
