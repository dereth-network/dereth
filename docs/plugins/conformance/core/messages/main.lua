-- The message stream: every message the client received or sent, decoded and read only. An
-- appraisal is one message sent and one received; each is heard by its name and as any message.
local seen = { received = 0, sent = 0 }
messages.on("any", function(m)
	seen[m.dir] = seen[m.dir] + 1
end)
messages.on("Item_Appraise", function(m)
	print("sent", m.name, string.format("0x%04X", m.opcode), string.format("0x%08X", m.fields.target))
end)
messages.on("Item_SetAppraiseInfo", function(m)
	print("received", m.name, m.dir, string.format("0x%08X", m.fields.object), m.codec,
		m.fields.profile.success_flag)
end)
commands.add("id", "appraise the ring", function()
	print("stream version", messages.version)
	task.spawn(function()
		await(act.appraise(0x80000011))
		print("any", seen.sent >= 1, seen.received >= 1)
		print("send", messages.send == nil, rawget(messages, "send") == nil)
	end)
end)
