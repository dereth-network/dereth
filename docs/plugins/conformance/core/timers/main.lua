-- Timers run on the frame clock: a repeating one, a one-shot, and a task that sleeps.
local ticks = 0
local id
id = task.every(250, function()
	ticks = ticks + 1
	print("every", ticks)
	if ticks == 3 then task.cancel(id) end
end)
task.after(600, function() print("after") end)
task.spawn(function()
	print("sleeping")
	task.sleep(400)
	print("woke")
end)
