-- A script written for UtilityBelt's scripting, in a folder with no manifest: it reads its
-- character, the world and an object's properties as UtilityBelt's API answers them, and waits on
-- an action.
local RING = 0x80000011
print("state", game.State, game.State == ClientState.In_Game)
local me = game.Character.Weenie
print("me", me.Name, me.Id == game.CharacterId, me.ObjectClass)
local ring = game.World.Get(RING)
print("ring", ring.Name, ring.ObjectClass == ObjectClass.Jewelry, ring.Value(IntId.Value), ring.HasValue(IntId.Value))
print("missing", ring.Value(IntId.ItemWorkmanship), ring.Value(StringId.LongDesc, "none"))
local n = 0
for o in game.World.GetAll(ObjectClass.Jewelry) do
	n = n + 1
end
print("jewelry", n, #game.Character.Inventory, game.Character.GetInventoryCount("Gold Ring"))
local res = await(game.Actions.ObjectAppraise(RING))
print("appraised", res.Success, res.Error, ring.HasAppraisalData, ring.Value(IntId.ItemSpellcraft))
print("strings", ring.StringValues[StringId.Inscription], ring.IntValues[106])
game.Actions.ObjectAppraise(RING, nil, function(a)
	print("callback", a.Name, a.ObjectId == RING)
end)
sleep(500)
print("slept")
