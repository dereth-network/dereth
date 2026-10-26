-- UtilityBelt's other modules and globals over the plugin interface: the sandboxed files (the
-- script's own folder read only, its data folder read and written), JSON, dates and spans, regular
-- expressions, coordinates, marks in the world, enumerations and their flags, property tables by
-- typed id or number, vectors, and the client's message stream.
local acc = require("acclient")
local fs = require("filesystem")
local RING, DRUDGE = 0x80000011, 0x80000021

local data = fs.GetData()
data.WriteText("notes/a.txt", "hello")
print("data", data.ReadText("notes/a.txt"), data.FileExists("notes/a.txt"), #data.GetFiles("notes"))
local own = fs.GetScript()
print("own", own.FileExists("index.lua"), own.ReadLines("lines.txt")[2], own.CanWriteFile("x.txt"))
local ok, err = pcall(own.WriteText, "x.txt", "y")
print("read only", ok, string.find(tostring(err), "read only", 1, true) ~= nil)

local t = json.parse('{"a":[1,2,{"b":true}],"c":"d"}')
print("json", t.a[3].b, t.c, json.serialize({ x = 1, y = { 1, 2 } }))

local started = DateTime.UtcNow
sleep(1500)
local span = DateTime.UtcNow - started
print("span", span.TotalSeconds >= 1.5 and span.TotalSeconds < 2, TimeSpan.FromMinutes(2).TotalSeconds)

local m = Regex.Match('Bob tells you, "buffs"', "^(?<who>\\w+) tells you")
print("regex", m.Success, m.Groups["who"].Value, Regex.IsMatch("abc", "B", RegexOptions.IgnoreCase))

local here, north = acc.Coordinates.new(42.0, 33.0, 0), acc.Coordinates.new(42.1, 33.0, 0)
print("coords", math.floor(here.DistanceToFlat(north) + 0.5), here.HeadingTo(north))

local arrow = acc.DecalD3D.NewD3DObj()
arrow.SetShape(acc.DecalD3DShape.VerticalArrow)
arrow.Color = 0xFFFF0000
arrow.Anchor(DRUDGE, 0.2, 0, 0, 0)
arrow.Visible = true
acc.DecalD3D.MarkObjectWith3DText(DRUDGE, "Here", "Arial", 0xFF00FF00)

print("enum", tostring(IntId.Value), IntId.Value.ToNumber(), ObjectType.Creature, CombatMode.Melee + CombatMode.Missile, (CombatMode.Melee + CombatMode.Missile).HasFlags(CombatMode.Missile), IntId[19] == IntId.Value)
local ring = game.World.Get(RING)
print("dict", ring.IntValues[19], ring.IntValues[IntId.Value], ring.IntValues.ContainsKey(IntId.Value), ring.DataValues[DataId.Icon] ~= nil)

local v = Vector2.new(1, 2) + Vector2.new(3, 4)
print("vector", v.X, v.Y, tostring(Vector3.new(1, 2, 3)))

game.Messages.Incoming.Item_SetAppraiseInfo.Add(function(e)
	print("message", e.Name, e.Data ~= nil)
end)
await(game.Actions.ObjectAppraise(RING))
