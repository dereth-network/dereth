-- HTTPS to the manifest's domains only: a listed domain's subdomain answers; a request to another
-- domain or another port fails, saying the domain is not listed; a plain http URL and an address
-- are not requests at all; a missing file fails.
task.spawn(function()
	local r = await(net.fetch("https://news.example.org/board.txt"))
	print("board", r.status, r.body)
	print("missing", pawait(net.fetch("https://example.org/none")))
	print("other domain", pawait(net.fetch("https://example.com/board.txt")))
	print("other port", pawait(net.fetch("https://example.org:8443/board.txt")))
end)
local ok, err = pcall(net.fetch, "http://example.org/board.txt")
print("plain http", ok, string.find(tostring(err), "https:// URLs only", 1, true) ~= nil)
ok, err = pcall(net.fetch, "https://127.0.0.1/board.txt")
print("address", ok, string.find(tostring(err), "https:// URLs only", 1, true) ~= nil)
