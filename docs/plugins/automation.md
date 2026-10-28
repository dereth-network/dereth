# Automation: combat and metas

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

Automation is a plugin acting with no player gesture behind it. Every such action needs its
domain's `auto.<domain>` permission and a world that allows the domain without a gesture (see
[permissions](permissions.md) and [the policy](policy.md)); a world may also turn all automation
off while the character is a player killer (`pk_automation = false`), and ask that plugins see
only what the radar shows (`read_parity = true`). The host enforces both: an automated action of
a player killer in such a world fails with `denied_by_world` (one the player asked for does not),
and `game.world:find` then answers only what the radar lists and what the character holds.

## Combat

| call | sends | settles when | timeout | domain |
|---|---|---|---|---|
| `combat.set_mode("combat" or "peace")` | the combat toggle, when the mode differs | the character is in (or out of) a combat mode; the value is the mode number | 5 s | `combat` |
| `combat.attack(target, { height, power })` | a selection of the target, then the combat window's power, height and release | a swing: attack-done (`Combat_HandleAttackDoneEvent`, `0x01A7`) with `0` (the sequence goes on) or with `ActionCancelled` (`0x0036`, the sequence's end), or the next swing's commence-attack (`Combat_HandleCommenceAttackEvent`, `0x01B8`); see below | 20 s | `combat` |
| `combat.stop()` | `Combat_CancelAttack` (`0x01B7`) when a swing is out; a swing still charging, or waiting for the last swing's answer, is dropped unsent | at the next frame; the plugin's attacks in flight or queued fail as `stopped` | none | none for the plugin's own attack (stopping is always allowed); `combat` to stop an attack it did not start |

`height` is `high`, `medium` (the default) or `low`; `power` is from 0 to 1 (default 0.5). The
swing goes out when the client's power bar reaches the power, as a player's click would.
`combat.fighting()` is whether the character is in a combat mode. `game.world:find` takes
**How an attack settles.** The server sends no answer of its own to an attack request. Each
swing ends with attack-done: `0` when the attack sequence goes on (repeating attacks, the next
swing announced by commence-attack), `ActionCancelled` (`0x0036`) when the sequence ends, after its
last swing or at once after a refusal (`Communication_WeenieError`, such as `0x001D` for too busy).
A request made while a sequence runs joins it and is answered by the sequence's next swing; a
target out of reach is walked to first (the server allows 15 s). So an attack is done on a swing
ended or begun, or on the sequence's end with no refusal; it fails `refused (0x…)` with the
refusal's code, `failed (0x…)` on another attack-done code, and `refused by the client` when the
client does not charge or send it (not in a combat mode, no attackable target, not ready) or gives
up charging it. A completed attack's value says how it ended: `{ ended = "done" }` (attack-done
`0`), `{ ended = "commenced" }` (the next swing begun) or `{ ended = "cancelled" }` (the sequence's
end with no refusal; an attack on a target that has just fallen ends this way at once).

**An attack goes only at its target.** The swing goes at whatever is selected when the power bar
reaches the power, and the selection can change while the bar charges: when a target that has
fallen is taken out of the world, the client's automatic targeting selects the next monster, which
may be across the room. So an attack whose target stops being the selection before it is answered
fails `target_gone`: its swing is dropped (or, gone out already, cancelled with
`Combat_CancelAttack`), never sent at the new selection. A plugin chooses its next target itself.

`monster = true` (creatures the character can attack that are not players, so not a town's
people) and `within = <metres>` (from the character).

## Metas: `dereth.meta`

A meta is a state machine of rules, each a condition and an action, in the manner of VTank's
metas. Every `every` milliseconds (293 by default) the meta checks its current state's rules in
order and runs the action of each whose condition holds. With `once = true` (VTank's rule, which an
imported meta uses) each rule runs at most once per stay in its state, and setting the meta's state
to the state it is in enters that state again, so its rules run again. A rule whose action changes
or re-enters the state ends that check.

VTank's rule is as its documentation states it ("Virindi Tank Meta System", virindi.net wiki,
*Rules and States*): "As the macro runs, it looks at all the rules which haven't run yet with the
same state as the current state. If the condition is true, the action listed in the rule is run.
Each rule only happens once per time the macro is in a particular state", and conditions "are
evaluated approximately three times a second while the macro is enabled (once every 293
milliseconds and whenever the macro needs to decide what to do next)". Community metas re-enter a
state by setting it to itself to run its rules again. The documentation does not say whether a
state change ends a check; here it does, and so does the meta's stopping (another meta taking its
place, say). A meta checks on its timer; the checks "whenever the macro needs to decide what to do
next" are the engine's to make: a plugin that drives a macro calls `m:check()` as its macro decides
(the Tank sample does). A condition that raises an error counts as false, and the error is told in
the chat.

```lua
local meta = require("dereth.meta")
local m = meta.new{
  start = "Idle",
  states = {
    Idle = {
      { meta.pack_slots_le(3), meta.set_state("Full") },
      { meta.monsters_within(1, 25), meta.set_state("Fight") },
    },
    Fight = {
      { meta.died(), meta.set_state("Dead") },
      { meta.always(), meta.watchdog_set("Idle", 5, 30) },
    },
    Dead = { { meta.portal_exited(), meta.set_state("Idle") } },
    Full = { { meta.always(), meta.chat("/say pack full") } },
  },
}
m:on_state(function(new, old) print(old, "->", new) end)
m:start()   -- and m:stop(); m:check() runs one check; m:set_state(name)
-- m:running() is whether its timer runs; m:restart_clock() counts the seconds in its state from now
```

| conditions | true when |
|---|---|
| `always()`, `never()` | always, never |
| `all(c, …)`, `any(c, …)`, `no(c)` | all, any, not |
| `chat_match(pattern)` | a chat line since the last check matches the Lua pattern (captures in `ctx.captures`) |
| `chat_regex(pattern)` | a chat line since the last check matches the [pattern](#patterns) (groups in `ctx.captures`) |
| `chat_capture(pattern [, colors])` | as `chat_regex`, and each named group `n` is kept in the meta's variable `capturegroup_n`; with `colors` (chat line types, a list or their numbers separated by semicolons, as VTank's color string), only lines of those types match, and the matched line's type is kept in `capturecolor` |
| `pack_slots_le(n)` | the main pack has at most `n` free slots |
| `item_count_le(name, n)`, `item_count_ge(name, n)` | the character holds at most / at least `n` of the named item |
| `monsters_within(n, metres [, name])` | at least `n` monsters within the distance (whose names match the pattern, when given) |
| `no_monsters_within(metres)` | no monster within the distance |
| `landblock(id)`, `landcell(id)` | the character is in the landblock (the cell's high half) or the cell |
| `vendor_open()`, `vendor_closed()` | a vendor's window is open, or none is |
| `vital_le(vital, fraction)` | `health`, `stamina` or `mana` is at or below the fraction of its maximum |
| `spell_time_le(spell, seconds)` | the spell is not on the character, or has at most that long left |
| `spell_time_ge(spell, seconds)` | the spell is on the character with at least that long left |
| `burden_ge(percent)` | the character carries at least that share of what it can |
| `seconds_in_state_ge(seconds)` | the meta has been in its state that long, counted from its entry or the meta's last start (or `m:restart_clock()`) |
| `seconds_in_state_persist_ge(seconds)` | the meta has been in its state that long, counted from its entry however often it was stopped and started (VTank's "(P)" condition) |
| `navigating()` | a movement is running |
| `route_empty()` | the meta walks no route |
| `route_distance_ge(metres)` | the character is at least that far from every point of the route the meta walks |
| `portal_entered()`, `portal_exited()` | the character entered, or left, portal space since the last check |
| `in_portal()` | the character is in portal space |
| `died()` | the character died since the last check |
| `need_to_buff()` | the meta's `need_to_buff(ctx)` function (given to `meta.new`) answers true |
| `expr(fn or text)` | `fn(ctx)` answers true, or the [expression](#expressions) is true |

| actions | does |
|---|---|
| `none()` | nothing |
| `set_state(name)` | changes the state |
| `call_state(name [, back])`, `return_state()` | enters a state and later returns to the one it came from |
| `chat(line)`, `print(text)` | types a command (a line beginning `/` or `@`) or says the line (`chat.send`); prints locally |
| `chat_expr(text)` | as `chat`, the line being the expression's value |
| `run(text)` | runs an expression for what it does |
| `walk(route, opts)`, `stop_nav()` | walks a [`dereth.nav`](nav.md) route (once at a time), stops walking |
| `watchdog_set(state, metres, seconds)`, `watchdog_clear()` | if the character does not move `metres` within `seconds`, enter `state` (re-entering it when it is the current one); the watchdog ends when cleared, when it fires, or when the state changes. Setting the same watchdog again keeps its clock |
| `set_option(name, value)`, `get_option(name, variable)` | sets one of the meta's options (a plugin hears it with `m:on_option(fn)`), or copies one into a variable |
| `create_view(name, xml)`, `destroy_view(name)`, `destroy_all_views()` | shows a [meta view](#meta-views) as a window, or closes it |
| `multiple(a, …)` | runs each in turn |
| `lua(fn)` | runs `fn(ctx)` |

`ctx` is `{ meta, chat, chat_types, events, state_seconds, state_seconds_persist, captures }`
(`chat_types` the type number of each line of `chat`). A meta's variables are
`m.vars`; its options are `m.options` (names in lower case). Every action a meta runs is the
plugin's own action, gated as any other.

## VTank meta files

`meta.load_met(text [, opts])` reads a VTank `.met` file (as `files.package` answers it, or its
rules as `meta.read_met` answers them) and
answers a meta checked by VTank's rule (`once = true`: each true rule runs once per stay in its
state). `opts.start`
picks the first state (else `Default`, else the first rule's), `opts.every` the period,
`opts.need_to_buff` the buff test, `opts.route_range` how near an embedded route's points count
as reached (3 m). What the import could not carry is listed, once each, in the meta's
`unsupported`: a condition kind it does not know (a dialog's appearance, for one) never holds, an
action it does not know does nothing, and an expression that names a function this host does not
support is listed with it. Monster priorities, which live in VTank's own settings, count any
monster unless the meta drives an engine that knows them.

**A meta driving an engine.** VTank runs a meta on top of its macro: an embedded route becomes the
macro's route, its options switch the macro's parts, its VTank commands (`/vt opt set …`, `/vt nav
load …`) are the macro's. `opts.engine` (for `meta.new` and `meta.load_met`) is that macro, a table
of functions, each optional:

| function | the meta uses it for |
|---|---|
| `route(route)` | an embedded route ("walk") is handed to the engine instead of walked by the meta; `route(nil)` for "stop navigation" |
| `route_empty()` | the route-empty condition |
| `route_distance()` | the distance-to-any-route-point condition: metres, or nil for no route |
| `option(name)` | "get option": the engine's setting |
| `command(line)` | a chat action's command: true when the engine answered it (it is then not typed) |
| `priority(obj)` | the monster-priority count condition: a monster's priority |

"Set option" still reaches the plugin through `on_option`.

The file is a typed table of rules (condition kind, action kind, their data, and the state): the
reader is in the host, and the rule kinds map onto the conditions and actions above. Embedded
routes are read as [`.nav` files](nav.md), a follow route too (walked by following its player, or
handed to the engine). A VTank option a meta sets (`EnableCombat`,
`EnableNav`, …) is only recorded: what it means is the plugin's to decide, through `on_option`.

**A meta as data, and written.** `meta.read_met(text)` answers a `.met` file's rules as data, for
a plugin that shows or changes them: `{ rules = { { state, condition = { kind, data }, action =
{ kind, data } }, … } }` in file order. The kinds are those of the tables above, by the names the
conditions and actions have there (`monster_count_within`, `set_state`, `walk`, …); `all`, `any`
and `no` (VTank's Not) hold a list of conditions, `multiple` (VTank's All) a list of actions. A
kind's data is a number, a text, or named fields as VTank names them (`n`, `c`, `r` for a monster
count's name, count and distance; `e` an expression's text; `o` and `v` an option and its value);
an embedded route's is `{ name, route, text }`, the route as `dereth.nav` reads it and the text it
was read from. A kind this reader knows only by its number is named `unknown:<n>`.
`meta.write_met(file)` answers the text of a `.met` file holding those rules, as VTank writes one
(each value on a line of its own, typed, lines ended `\r\n`; a meta view's markup running straight
into the next line): read again, it is the same rules, and anything read writes back as it was.
An embedded route is written from its `text`, so a route a plugin embeds is a `.nav` file's text
with the name to show for it.

## Meta views

A meta view is VTank's view markup: a `<view title=…>` with `<control>`s. `create_view` shows it
as a window with id `view:<name>`: the controls in rows by their height on the page, each row left
to right; a `button` is a button, a `checkbox` a toggle, text a label, and any other kind a label
naming it. A click on a button enters its `setstate` state, then runs its `actionexpr`
expression.

## Expressions

`require("dereth.expr")` evaluates expressions in the language VTank metas use:
`expr.eval(text [, env])`, or `expr.compile(text)` once and the result called with an `env`;
`expr.env{ vars, pvars, gvars }` makes one over plain tables. A meta's expressions use its own
variables: `vars` for the run, `pvars` kept in `store.character` and `gvars` in `store.global`.

- Values are numbers, text, `true` and `false`, lists, dictionaries, coordinates, objects and
  stopwatches. Text is written in backquotes (`` `a b` ``) or bare; a backslash takes the next
  character as it is (`\/t`).
- Operators, loosest first: `;` (in sequence), `||`, `&&`, the bitwise `|`, `^` and `&`, the
  comparisons `== != < > <= >=`, `#` (the left side, a [pattern](#patterns), matches the right
  side's text, without regard to case), the shifts `<< >>`, `+ -` (`+` joins text when either side
  is text), `* / %`, then `!`, `-` and `~` before a value. `&` is a bitwise and where a number or
  a function call follows it, else a variable (below).
- After a value, `{i}` is an item of a list or a character of text (from 0; a negative counts from
  the end), `{key}` a dictionary's value, and `{from:to}` a slice of a list or text (either end
  may be left out). An item that is not there is `0`.
- `$name` is a run variable, `@name` one kept for the character and `&name` one kept on this
  machine (UtilityBelt's spelling); `$name=value` sets it.
- A meta's text left with a bracket open at its very end is closed there, and a stray closing
  bracket at its very end is let be, as VTank and UtilityBelt let them be.
- Functions are called as `name[args]`: the variable functions (`getvar`, `setvar`, `testvar`,
  `touchvar`, `clearvar`, `clearallvars`, and the `p` and `g` forms), `istrue`, `isfalse`, `iif`,
  `cstr`, `cnumber`, `strlen`, `echo`, `chatbox`, `getcharintprop`, `getcharstringprop` (1, the
  name), `getcharvital_current`, `getcharvital_base`, `getcharburden`, `getplayerlandcell`,
  `getplayercoordinates`, `coordinatedistancewithz`, `coordinatedistanceflat`,
  `coordinatetostring`, `coordinategetns`, `coordinategetwe`, `isportaling`,
  `getinventorycountbyname`, `wobjectgetplayer`, `wobjectgetselection`, `wobjectgetopencontainer`,
  `wobjectfindbyid`, `wobjectgetname`, `wobjectgetid`, `wobjectgetobjectclass`,
  `wobjectgetphysicscoordinates`, `wobjectfindnearestmonster`, `wobjectfindnearestbyobjectclass`,
  `wobjectfindnearestbynameandobjectclass`, `wobjectfindininventorybyname`, `actiontryselect`,
  `actiontryuseitem`, `actiontryapplyitem`, `actiontrycastbyid`, `actiontrycastbyidontarget`,
  `listcreate`, `listadd`, `listcontains`, `listcount`, `listgetitem` (from 0), `listremove`,
  `listclear`, `getgametime`, and UtilityBelt's (its expression reference names them; each answers
  as it says, from what this host knows):
  - lists: `listinsert`, `listremoveat`, `listindexof`, `listlastindexof`, `listcopy`,
    `listreverse`, `listpop`, `listfromrange`, and `listfilter`, `listmap`, `listreduce`,
    `listsort`, whose second argument is an expression (text) run per item with `$0` its count,
    `$1` the item and, for `listreduce` and `listsort`, `$2` the value so far or the other item;
  - dictionaries: `dictcreate`, `dictgetitem`, `dictadditem`, `dicthaskey`, `dictremovekey`,
    `dictkeys`, `dictvalues`, `dictsize`, `dictclear`, `dictcopy`;
  - text and math: `tostring`, `getregexmatch`, `cstrf` (.NET's standard formats `C`, `D`, `E`,
    `F`, `G`, `N`, `P`, `R` and `X`, each with a precision, as `N2`; and custom ones of `0`, `#`,
    `,`, `.` and `%` with text around them, as `0.00` or `#,##0`), `ord`, `chr`,
    `hexstr`, `floor`, `ceiling`, `round`, `abs`, `sqrt`, `sin`, `cos`, `tan`, `sinh`, `cosh`,
    `tanh`, `asin`, `acos`, `atan`, `atan2`, `randint`;
  - logic: `ifthen` (only the chosen branch runs), `exec` (text run as an expression),
    `delayexec` (after so many milliseconds), `clearallpvars`, `clearallgvars`;
  - stopwatches: `stopwatchcreate`, `stopwatchstart`, `stopwatchstop`, `stopwatchelapsedseconds`;
  - the character: `getchardoubleprop`, `getcharquadprop`, `getcharboolprop`,
    `getcharstringprop` (any key), `getcharvital_buffedmax`, `getcharskill_traininglevel` (0 to
    3), `getcharskill_base`, `getcharskill_buffed`, `getcharattribute_base`,
    `getcharattribute_buffed`, `vitae` (the penalty, percent), `getcombatstate`,
    `setcombatstate` (an action: combat or peace), `getbusystate`, `getequippedweapontype`;
  - places and movement: `getplayerlandblock`, `coordinategetz`, `coordinateparse`,
    `getheading`, `getheadingto`; `setmotion`, `getmotion` and `clearmotion` for `Forward`, the
    run lock, the one motion a plugin holds (the others answer false);
  - time and the world: `getgameyear`, `getgamemonth`, `getgamemonthname`, `getgameday`,
    `getgamehour`, `getgamehourname`, `getgameticks`, `getisday`, `getisnight`,
    `getminutesuntilday`, `getminutesuntilnight` (the client's calendar), `getworldname`,
    `getaccounthash`, `getdatetimelocal`, `getdatetimeutc` (a `%` format), `getunixtime`;
  - spells: `getisspellknown`, `getcancastspell_hunt`, `getcancastspell_buff` (known, and the
    buffed skill of the spell's school at least the spell's difficulty and the threshold more:
    the engine's `SpellDiffExcessThreshold-Hunt` or `-Buff` option, else VTank's 25 or 5; a known
    spell counts where the host tells no skill or difficulty),
    `getspellexpiration`, `getspellexpirationbyname`, `spellname`, `spelldata`,
    `getknownspells`;
  - quests: `testquestflag`, `getqueststatus`, `getquestktprogress`, `getquestktrequired`,
    `isrefreshingquests`, read from the server's quest list lines as they arrive in the chat
    (`expr.refresh_quests()` asks for them with `/myquests`, which needs `chat.send`);
  - objects: `wobjectfindnearestdoor`, `wobjectfindnearestbytemplatetype` (a template type is
    its kind's number, `wcid`), `wobjectfindininventorybytemplatetype`,
    `wobjectfindininventorybynamerx`, `wobjectfindall`, `wobjectfindallinventory` and their
    `…byobjectclass`, `…bytemplatetype` and `…bynamerx` forms, `wobjectfindalllandscape` (on
    the ground) and its forms, `wobjectfindallbycontainer`, `wobjectgettemplatetype`,
    `wobjectgetintprop`, `wobjectgetdoubleprop`, `wobjectgetboolprop`, `wobjectgetstringprop`,
    `wobjectgethealth` (0 to 1, or -1), `wobjectgetisdooropen`, `wobjectgetspellids`,
    `wobjectgetactivespellids` (the character's), `wobjecthasdata`, `wobjectrequestdata` (an
    appraisal), `wobjectlastidtime` (when its appraisal arrived, on the plugin's clock),
    `wobjectisvalid`, `getobjectinternaltype`;
  - inventory: `getitemcountininventorybyname`, `getitemcountininventorybynamerx`,
    `getinventorycountbytemplatetype`, `getfreeitemslots`, `getfreecontainerslots`,
    `getcontaineritemcount`;
  - actions: `actiontrygiveitem`, `actiontryequipanywand`;
  - the fellowship: `getfellowshipstatus`, `getfellowshipname`, `getfellowshipcount`,
    `getfellowshipleaderid`, `getfellowid`, `getfellowname` (by name order, from 0),
    `getfellowshiplocked`, `getfellowshipisleader`, `getfellowshipisopen`,
    `getfellowshipisfull`, `getfellowshipcanrecruit`, `getfellownames`, `getfellowids`;
  - the interface: `statushud`, `statushudcolored` (a HUD of key and value lines), and for a
    meta's views `uiviewexists`, `uiviewvisible`, `uigetcontrol`, `uisetlabel`;
  - the macro: `vtgetmetastate`, `vtsetmetastate` (enters the meta's state; false when it has
    none of that name), `vtgetsetting`, `vtsetsetting` (the meta's options, as a meta's
    `set_option` records them; `vtgetsetting` reads the engine's option first, for a meta that
    drives one), `uboptget`, `uboptset` (recorded under `ub:<name>`).

  Not answered, so an error when compiled: `actiontrygiveprofile`, `chatboxpaste`,
  `componentname`, `componentdata`, `uisetvisible`, `netclients`, `ustadd`, `ustopen`,
  `ustsalvage`.
- A function outside this list is an error when the expression is compiled, and so is text that
  does not parse. The `actiontry…` functions start the plugin's action and answer at once.

## Patterns

`regex.match`, the chat conditions and `#` use regular expressions in the common dialect:
classes, groups, named groups `(?<name>…)`, alternation, repetition and anchors. A backslash before
`<` or `>` is a plain angle bracket.

## Combat helpers: `dereth.combat`

| call | does |
|---|---|
| `combat.stop()` (the namespace) | stops attacking: a swing already sent is cancelled with the server, one still charging is dropped, and the plugin's attacks in flight or queued fail as `stopped`; settles at the next frame |
| `fight.heal{ vital = "health", below = 0.5, spells = { id, … }, kits = { name, … } }` | at or below the mark, casts the first known spell of the list on the character as a heal, else uses the first kit held; answers the promise, or nil when nothing is to be done |
| `fight.ready_magic{ caster = name }` | inside a task: into magic stance, where spells are cast; a caster held is kept, else the one named (else the first in the pack) is taken up in place of the weapon held; answers `true` and the weapon put away (or nil), or `false` and why (`no_caster`, or the failed step's reason) |
| `fight.take_up(item)` | inside a task: wields `item` (an object, or a name in the pack) in place of the weapon held, in peace stance; answers `true`, or `false` and why |
| `fight.debuffer{ spells = { id or { id, recast }, … }, recast = 60 }` | `d:next(target)` casts the first known spell not landed on the target within its recast time, and answers the cast's promise (nil when all are up); `d:forget([target])` |

(`local fight = require("dereth.combat")`.) Healing, by a kit or by a Life spell cast as a heal,
is the `heal` domain; a debuff on another is `combat`.
