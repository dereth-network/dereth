-- What a plugin cannot reach.
print("io", io == nil, "loadstring", loadstring == nil, "execute", os.execute == nil)
print("library read-only", (pcall(function() string.rep = nil end)))
print("api read-only", (pcall(function() game.player = nil end)))
print("module outside package", (pcall(require, "os")))
print("own module", require("helper").answer)
