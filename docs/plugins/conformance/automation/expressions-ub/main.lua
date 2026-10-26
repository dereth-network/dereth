-- UtilityBelt's additions to VTank's expression language: dictionaries, list and string indexing
-- and slicing, the list library with item expressions, math and bitwise operators, the kept and
-- shared variables, stopwatches, lazy `ifthen`, `exec`, the calendar, and the meta's own state.
local expr = require("dereth.expr")
local meta = require("dereth.meta")
local todo = {}
local env
local function show(text)
	local ok, v = pcall(expr.eval, text, env)
	if ok then
		print(text, expr.str(v))
	else
		print(text, "error", v)
	end
end
on("session.login", function()
	local m = meta.new({ states = { Default = {}, Hunt = {} }, start = "Default" })
	env = m.env
	task.every(50, function()
		local text = table.remove(todo, 1)
		if text ~= nil then
			show(text)
		end
	end)
end)
for _, text in ipairs({
	"$d=dictcreate[a,1,b,2]",
	"dictgetitem[$d,b]",
	"$d{a}+10",
	"dictkeys[$d]",
	"dicthaskey[$d,c]",
	"$d{zzz}",
	"$l=listfromrange[1,5]",
	"$l{0}",
	"$l{-1}",
	"$l{1:3}",
	"listpop[$l]",
	"listindexof[$l,3]",
	"listfilter[$l,`$1%2==0`]",
	"listmap[$l,`$1*10`]",
	"listreduce[$l,`$2+$1`]",
	"listsort[listcreate[3,1,2]]",
	"listreverse[$l]",
	"`hello`{1:3}",
	"floor[2.7]+ceiling[2.1]+round[2.5]+abs[-1]",
	"hexstr[255]",
	"5&3",
	"5|2",
	"1<<4",
	"6^3",
	"cstrf[1234.5,`N1`]",
	"cstrf[0.256,`P1`]",
	"cstrf[42,`D5`]",
	"cstrf[255,`X4`]",
	"cstrf[12345.678,`E2`]",
	"cstrf[9.5,`C`]",
	"cstrf[3.14159,`0.00`]",
	"cstrf[1234567,`#,##0`]",
	"cstrf[0.5,`0%`]",
	"ifthen[0,setvar[side,1],setvar[side,2]];getvar[side]",
	"ifthen[1,1,setvar[other,9]];testvar[other]",
	"@kept=3",
	"getpvar[kept]",
	"&shared=4",
	"getgvar[shared]",
	"$sw=stopwatchcreate[];stopwatchstart[$sw];stopwatchelapsedseconds[$sw]>=0",
	"vtsetmetastate[Hunt];vtgetmetastate[]",
	"vtsetmetastate[Nowhere]",
	"vtsetsetting[RangeMax,30];vtgetsetting[rangemax]",
	"getgamehour[]>=0&&getgamehour[]<16",
	"setvar[t,getvar[a]+1",
	"getvar[t]==1]",
	"exec[`1+1`]",
	"getobjectinternaltype[wobjectgetplayer[]]",
	"getfellowshipstatus[]",
	"getcharskill_traininglevel[999]",
}) do
	table.insert(todo, text)
end
