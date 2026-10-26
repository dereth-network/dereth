-- A handler that always fails: the host logs each failure and suspends the plugin.
task.every(100, function() error("always") end)
