-- VTank's expression language, evaluated against the game and a set of variables.
local expr = require("dereth.expr")
local env = expr.env()
local function show(text)
	local ok, v = pcall(expr.eval, text, env)
	if ok then
		print(text, expr.str(v))
	else
		print(text, "error", v)
	end
end
-- One expression a tick, each well inside a callback's time budget.
local todo = {}
local function queue(text)
	table.insert(todo, text)
end
on("session.login", function()
	task.every(50, function()
		local text = table.remove(todo, 1)
		if text ~= nil then
			show(text)
		end
	end)
end)
do
	queue("1+2*3")
	queue("(1+2)*3")
	queue("`a`+`b`+1")
	queue("7/2")
	queue("-4+10%3")
	queue("1==1&&2>1")
	queue("1==2||3<=2")
	queue("!0")
	queue("getvar[x]==0")
	queue("$x=5")
	queue("$x+1")
	queue("setvar[name, Bob]")
	queue("getvar[name]")
	queue("testvar[nope]")
	queue("iif[getvar[x]>3,big,small]")
	queue("\\/t +getcharstringprop[1]+\\, hi\\.")
	queue("`(fol|lead)low`#`Please follow me`")
	queue("(fol|lead)low#`Please follow me`")
	queue("(xyz)#abc")
	queue("$l=listcreate[a,b]")
	queue("listadd[$l,c]")
	queue("listcount[$l]")
	queue("listcontains[$l,b]")
	queue("listgetitem[$l,2]")
	queue("getgvar[qnxc-chars]")
	queue("wobjectgetname[wobjectfindnearestbynameandobjectclass[27,Corpse of a Drudge]]")
	queue("wobjectgetname[wobjectfindnearestmonster[]]")
	queue("coordinatedistancewithz[getplayercoordinates[],wobjectgetphysicscoordinates[wobjectfindnearestmonster[]]]<30")
	queue("getplayerlandcell[]>0")
	queue("isportaling[]")
	queue("getinventorycountbyname[Pyreal]")
	queue("nosuchfunction[]")
	queue("1+")
end
