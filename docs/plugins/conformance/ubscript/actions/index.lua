-- UtilityBelt's action queue over the plugin interface's actions: an immediate action at once,
-- the rest one at a time and the highest priority first, each ending with its error and details,
-- its callback heard, a group run in its own order, and chat lines typed at the interface's pace.
local RING, SWORD, PACK, NOBODY = 0x80000011, 0x80000021, 0x80000031, 0x80000099
local function done(a)
	print("done", a.Name, a.Success, a.Error, a.ErrorDetails)
end
-- Queued together: the move goes first (it was alone when it came), then the wield (priority
-- 20000) before the give (9000), and the appraisal (immediate) does not wait.
local move = game.Actions.ObjectMove(RING, PACK, 0, false, nil, done)
local give = game.Actions.ObjectGive(RING, NOBODY, nil, done)
-- A drop the server never answers ends at the script's own timeout, not tried again.
local quick = ActionOptions.new()
quick.TimeoutMilliseconds = 1000
quick.MaxRetryCount = 0
local drop = game.Actions.ObjectDrop(SWORD, quick, done)
local wield = game.Actions.ObjectWield(SWORD, EquipMask.MeleeWeapon, nil, done)
local look = game.Actions.ObjectAppraise(RING, nil, done)
print("queued", #game.ActionQueue.Queue, game.ActionQueue.Contains(give), move.IsStarted, give.IsStarted)
await(drop)
print("drop", drop.Success, drop.Error == ActionError.TimedOut, drop.CurrentRetryCount)
-- Options: a name and a priority of the script's own.
local opts = ActionOptions.new()
opts.Name = "mine"
opts.priority = 50000
opts.MaxRetryCount = 0
local named = ObjectAppraiseAction.new(RING, opts)
print("named", named.Name, named.Priority, named.ObjectId == RING, named.IsStarted)
game.ActionQueue.Add(named)
print("awaited", await(named).Success, named.IsFinished)
-- A group runs in its own order, as one action.
local group = game.Actions.RunAllOrdered({ ObjectAppraiseAction.new(SWORD), ObjectAppraiseAction.new(RING) })
group.Await()
print("group", group.Success, #group.Actions)
-- A removed action never runs.
local blocker = game.Actions.ObjectUse(RING)
local removed = game.Actions.ObjectAppraise(SWORD, ActionOptions.new({ Priority = 1 }))
game.ActionQueue.Remove(removed)
await(blocker)
print("removed", removed.IsStarted, game.ActionQueue.Contains(removed))
-- Chat lines: each sent, waiting while the interface's pace holds them back.
for i = 1, 5 do
	game.Actions.InvokeChat("/e waves " .. i)
end
sleep(6000)
print("chatted")
