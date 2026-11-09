-- {{name}}: says hello, in a small window and with /{{command}} in chat.

if host.has("ui.window") then
	local win = ui.window({
		id = "main",
		title = "{{name}}",
		width = 20,
		ui.label({ id = "greeting", text = "Hello!" }),
		ui.button({ id = "wave", text = "Wave" }),
	})
	on("ui.clicked", function(e)
		if e.id == "wave" then
			win:get("greeting").text = "Hello again!"
		end
	end)
end

commands.add("{{command}}", "/{{command}} says hello", function()
	chat.print("Hello from {{name}}.", "system")
end)
