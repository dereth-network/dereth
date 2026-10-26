-- A request to a listed domain from a plugin whose manifest does not name net.http fails, saying
-- so, and goes nowhere.
task.spawn(function()
	print("board", pawait(net.fetch("https://news.example.org/board.txt")))
end)
