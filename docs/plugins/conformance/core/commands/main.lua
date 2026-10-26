-- Claims /conf; the client's own /t (tell) cannot be claimed.
commands.add("conf", "conformance command", function(args, words)
	print("conf", args, #words)
	chat.print("conf answered " .. args)
end)
local ok, err = pcall(commands.add, "t", "", function() end)
print("claim t", ok, string.find(tostring(err), "client's own", 1, true) ~= nil)
