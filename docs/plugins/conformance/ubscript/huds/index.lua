-- A HUD drawn the immediate way, with UtilityBelt's views and ImGui modules, as a declared window:
-- each render builds the window's nodes; a click, a toggle, text typed, a number stepped, a choice
-- picked and a tab chosen reach the next render as ImGui reports them.
local ubviews = require("utilitybelt.views")
local im = require("imgui")
local ImGui = im.ImGui

local hud = ubviews.Huds.CreateHud("Counter")
hud.ShowInBar = true
hud.WindowSettings = im.ImGuiWindowFlags.AlwaysAutoResize

local count, debug, name, level, pick = 0, false, "Aldric", 5, 0
hud.OnRender.Add(function()
	ImGui.Text("Count: " .. count)
	if ImGui.Button("Add") then
		count = count + 1
		print("added", count)
	end
	ImGui.SameLine()
	if ImGui.Button("Reset##reset") then
		count = 0
	end
	local changed, v = ImGui.Checkbox("Debug", debug)
	if changed then
		debug = v
		print("debug", debug)
	end
	local typed, text = ImGui.InputText("Name", name, 32)
	if typed then
		name = text
		print("name", name)
	end
	local stepped, n = ImGui.InputInt("Level", level)
	if stepped then
		level = n
		print("level", level)
	end
	local picked, p = ImGui.Combo("Pick", pick, { "One", "Two", "Three" })
	if picked then
		pick = p
		print("pick", pick)
	end
	if ImGui.BeginTabBar("pages") then
		if ImGui.BeginTabItem("Main") then
			ImGui.Text("main page")
			ImGui.EndTabItem()
		end
		if ImGui.BeginTabItem("Other") then
			ImGui.TextColored(Vector4.new(1, 0, 0, 1), "other page")
			ImGui.EndTabItem()
		end
		ImGui.EndTabBar()
	end
	if ImGui.BeginTable("stats", 2) then
		ImGui.TableSetupColumn("Name")
		ImGui.TableSetupColumn("Value")
		ImGui.TableHeadersRow()
		ImGui.TableNextRow()
		ImGui.TableNextColumn()
		ImGui.Text("count")
		ImGui.TableNextColumn()
		ImGui.Text(tostring(count))
		ImGui.EndTable()
	end
	ImGui.ProgressBar(count / 10, Vector2.new(100, 10), "progress")
	if ImGui.IsItemHovered() then
		ImGui.SetTooltip("ten to go")
	end
end)
hud.Visible = true
print("visible", hud.Visible)
