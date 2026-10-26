-- Hides every line about the sparring golem.
on("chat.line", function(e)
	if string.find(e.text, "Sparring Golem", 1, true) then chat.hide(e) end
end)
