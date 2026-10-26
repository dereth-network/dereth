-- Hears each chat line the frame shows, with its kind and its type's number; prints a local line
-- of its own.
local n = 0
on("chat.line", function(e)
	n = n + 1
	if n <= 6 then print(e.line, e.kind, e.type, e.text) end
end)
chat.print("chat-lines loaded", "system")
