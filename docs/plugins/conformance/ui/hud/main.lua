-- A HUD is a window with no frame, placed by an anchor; its nodes change like a window's.
local hud = ui.hud{ id = "xp", anchor = "top_right", ui.label{ id = "rate", text = "0/h" } }
hud:get("rate").text = "12/h"
local ok, err = pcall(ui.hud, { id = "bad", anchor = "nowhere" })
print("bad anchor refused", ok, string.find(tostring(err), "anchor", 1, true) ~= nil)
ok, err = pcall(ui.window, { id = "w", { kind = "hud", id = "inner" } })
print("nested refused", ok)
ok, err = pcall(ui.hud, { id = "typed", ui.row{ ui.input{ id = "line" } } })
print("an input refused", ok, string.find(tostring(err), "hud holds no input", 1, true) ~= nil)
for _, kind in ipairs({ "textbox", "number", "choice", "slider" }) do
  ok = pcall(ui.hud, { id = "k", { kind = kind } })
  print(kind .. " refused", ok)
end
