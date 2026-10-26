-- A handler that never returns is stopped by the time budget each time, and then suspended.
task.every(100, function() while true do end end)
