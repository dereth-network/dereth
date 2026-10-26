# UtilityBelt scripts

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

Scripts written for UtilityBelt's Lua scripting (its `ubscript` API) run in Dereth as they are. A
script's folder (its `index.lua` and whatever else it holds) goes in the plugins folder, and the
host runs it with UtilityBelt's globals: `game`, `await`, `sleep`, `print`, `require`, the
enumerations (`IntId`, `ObjectClass`, `CombatMode`, …), the action classes
(`ObjectUseAction`, …), `ActionOptions`, `json`, `DateTime`, `TimeSpan`, `Regex`, `Vector2` …
`Vector4`, and UtilityBelt's modules through `require` (`utilitybelt.views`, `imgui`,
`acclient`, `filesystem`, `ubnet`, `http`). Everything the script asks of the game goes through
the plugin interface: its actions are the plugin's own, held to the world's policy and the
permissions as any plugin's are, and what the plugin interface cannot do is refused with an
error that says so.

The library that does this is the host's (`dereth.ubscript`, written in Lua over the plugin
interface), so a script runs in every host that runs Lua plugins, the browser's included.

## Installing a script

A folder holding `index.lua` and no `plugin.toml` is a UtilityBelt script: the host reads it as
a package whose manifest it makes itself, named for the folder, with the id `ubscript.<the
folder's name>` (lowercase, each run of other characters a `-`), `index.lua` its entry, active in
the world, naming no permission and needing no capability. A zip of such a folder is one too.

A package with a manifest of its own says `ubscript = true` ([manifest](manifest.md)); its entry
is `index.lua` when it names none. Such a package may name `contexts = ["pregame", "world"]`,
for a script that logs in, `[net] domains` for its HTTPS requests, and so on.

## When a script runs

A script whose package is active only in the world (the folder kind) starts when a character
enters the world, and ends when the character logs off: its `game.OnScriptEnd` handlers run,
and its windows, marks, timers, actions and event handlers go with it. At the next entry it
starts again, afresh: new globals, its modules loaded again. A script whose package is also
active before the world starts as soon as the host loads it and keeps running until it is
unloaded. Unloading a script (or reloading it) ends it the same way.

The script's top level runs as a coroutine, as UtilityBelt runs it, so it may `await` and
`sleep` there; each event handler and each action's callback runs as a coroutine of its own.
The plugin interface's time budget holds ([README](README.md#how-a-plugin-runs)): the script's
start comes in its plugin's load or in the entry telling the plugin it is active, so its top level
has the load's budget; each later entry into it (an event handed to its handlers, a render of its
HUD, a timer) has the host's budget for an entry, and a script that does a great deal at once
splits the work with `sleep`.

## The script API

### Names

A member is found whatever the case of its name and its underscores, as UtilityBelt's script
engine finds one: `game.World.GetLandscape`, `game.world.getLandscape` and
`game.world.get_landscape` are one. A method is called with `.` or `:` alike. A member the
library does not know is an error naming it; one it knows and cannot answer is an error saying
why (`… is not available here: …`).

### Collections

A list (`game.World.GetAll()`, `game.Character.Inventory`, `obj.Items`) is a Lua table indexed
from 1: `#`, `ipairs` and `list[1]` read it, `for obj in list do` walks its values as
UtilityBelt's lists are walked, and `list.Count` is its length. A dictionary
(`obj.IntValues`, `fellowship.Members`) is a table keyed as UtilityBelt keys it (property ids,
member ids); `pairs` walks it, `Count`, `ContainsKey(k)`, `Keys` and `Values` read it, and a
property table is also indexed by the property's plain number (`obj.IntValues[19]`).

### Enumerations

Every enumeration of UtilityBelt's script API is a global, its values by name
(`IntId.Value`), with `GetValues()`, `GetNames()`, `FromValue(n)` and `GetName(n)`.

The values of the enumerations the API hands out and takes to choose what to do are **typed**,
as UtilityBelt's are: the property ids (`IntId`, `Int64Id`, `FloatId`, `StringId`, `BoolId`,
`DataId`, `InstanceId`), `ObjectClass`, `SkillId`, `AttributeId`, `VitalId`, `ActionError`,
`ActionType`, `ClientState`, `CombatMode`, `ChatMessageType`, `ChatChannel`,
`AddRemoveEventType`, `MotionStance`, `SkillTrainingType`, `MagicSchool`, `DistanceType`,
`UseActionType`, `CastEventType`, `FellowshipEventType`, `ClientConfirmationType`,
`EndTradeReason` and `TradeSide`. A typed value prints as its name (`Melee, Missile` for flags
together), compares equal to the same value (`evt.Type == VitalId.Health`), orders by its
number, combines with `+` (flags), and has `ToNumber()`, `HasFlags(…)`, `AddFlags(…)` and
`RemoveFlags(…)`; it tells `obj.Value(IntId.Level)` (an integer) from
`obj.Value(StringId.Name)` (a string). Every other enumeration's values are plain numbers, so
they compare with the numbers properties hold (`obj.IntValues[IntId.MaterialType] ==
MaterialType.Ivory`).

**One difference.** A typed value is not a number: `obj.ObjectClass == 24` is false where
`obj.ObjectClass == ObjectClass.Player` is true. Compare typed values with typed values, or with
`value.ToNumber()`.

### The game, the world and the character

`game` answers the client's state (`game.State`), the character's id, the account, the world's
name and the character list, `game.Character`, `game.World`, `game.Actions`,
`game.ActionQueue`, `game.Messages` and its events. `game.Character` and `game.World` are nil
outside the world, as UtilityBelt's are.

A **world object** (`game.World.Get(id)`, `game.World[id]`) reads through to the game each time:
its `Name`, `ObjectClass` (UtilityBelt's classes, as loot profiles number them), `ObjectType`,
`Value(id [, default])` and `HasValue(id)` for each property type, the property tables
(`IntValues` …), its container, wielder, items, packs and equipment, its place and distances, and
its actions (`Use`, `UseOn`, `Appraise`, `Give`, `Move`, `Wield`, `Drop`, `Inscribe`, …). Its
properties are what the client knows of it: what its public description told every client, under
the game's property numbers, then what its last appraisal added, then its own qualities (the
player's character's). The character's world object also has its `Skills` (every skill, an
untrained one at 0), `Attributes` and `Vitals`.

`game.World` finds objects by class (`GetAll(ObjectClass.Monster)`), by type (a number), by name
or by a function, the nearest (`GetNearest`) and those on the ground (`GetLandscape`); it has the
open container, the selection, the vendor and where the character is (`PhysicsEnvironment`).
`game.Character` has the inventory (the main pack's items and the packs' items, not what is
worn) counted and searched by name, class or function, the fellowship and its members' figures,
the allegiance, the trade, the spell book (which spells are known, a spell's name, icon, school,
level, category, difficulty, mana and duration, and whether the character's skill reaches it),
the enchantments and their time left, the vitae, the burden and the combat mode.

### Actions

An action (`game.Actions.ObjectUse(id)`, or `ObjectUseAction.new(id)` added with
`game.ActionQueue.Add`) waits its turn in UtilityBelt's queue, which runs one action at a time,
the highest priority first (`ActionType`'s numbers, or the action's `ActionOptions.Priority`),
and an immediate action (an appraisal, a selection, a chat line, a sleep) at once beside it. Its
turn come, it is carried out with the plugin interface's actions ([actions](actions.md)), which
wait their own turn in the host's queue. It ends with `Success`, `Error` (an `ActionError`) and
`ErrorDetails`; its callback (the last argument of a `game.Actions` method) runs, `await(action)`
and `action.Await()` answer it, and `OnStarted` and `OnFinished` are heard. An action that timed
out or found the character too busy is tried again, up to its `MaxRetryCount` (5 when it names
none); any other failure ends it. Its time is the plugin interface's for that action, unless its
options name `TimeoutMilliseconds`. `game.Actions.RunAllOrdered{ … }` runs actions in their own
order as one. `game.ActionQueue.Queue` lists what waits, and `Remove` takes an action out before
it runs.

| plugin interface failure | `Error` |
|---|---|
| `timeout` | `TimedOut` (tried again) |
| `busy`, or the server's too-busy refusal | `TooBusy` (tried again) |
| `refused (0x…)`, `failed (0x…)` | `ServerError`, the reason's name in `ErrorDetails` |
| `refused by the client`, `denied_by_world` | `PreconditionFailed` |
| `stopped`, `replaced`, `interrupted` | `Cancelled` |
| `target_gone` | `InvalidTargetObject` |
| `not_in_magic_mode` | `InvalidCombatMode` |
| an error raised by the call itself | `Exception` |

`InvokeChat(line)` types a line (or says it), waiting while the plugin interface's pace for
chat (a line a second, three at once) holds it back. A line naming VTank's commands goes to the
bundled Tank where it has them (`/vt opt set <setting> <value>`, `/vto`, `/vt start`, `/vt stop`,
`/vt pause`, `/vt nav load <route>`, `/vt meta set <state>`, `/vtns`), and a line naming
UtilityBelt's own commands is answered here: `/ub mexec <expression>` (in the
[expression language](automation.md#expressions)), `/ub use` and `/ub uselp <name>` (the nearest
object of that name), `/ub follow <name>`, `/ub give <item> to <target>` and `/ub bc <line>`
(typed here, and in each of the player's other clients running the same script). Any other
`/ub` command is refused.

### Events

Each event of UtilityBelt's has `Add`, `Remove`, `Once` and `Until` (heard until the handler
answers true), and begins listening to the game when its first handler is added. They are heard
from the plugin interface's events ([events](events.md)): objects created and released, the
selection, containers opened and closed, the world's tick (a second), chat (`OnChatText`, and
`OnTell`, `OnLocalMessage`, `OnEmote`, `OnSoulEmote`, `OnFellowMessage`, `OnChannelMessage` with
the speaker's name and id and the words, from the client's message stream where the host has
it), a confirmation asked for, portal space entered and left, enchantments, vitals, experience,
luminance, level and vitae changing, a cast sent, the character's death, the fellowship's,
trade's and vendor's changes, the client's state changing, the script's end, and an object's
moves and its going. Setting `Eat = true` on a chat line's argument hides the line.
`game.OnRender2D` and `OnRender3D` are heard twenty times a second.

### HUDs and ImGui

`require("utilitybelt.views").Huds.CreateHud(name)` makes a HUD, and `require("imgui").ImGui` is
ImGui's API, as UtilityBelt's are. A HUD is a [declared window](ui.md): each time it renders, its
`OnRender` handlers call ImGui and the calls build the window's nodes; when the shape of what
they built is what it was, only the properties that changed are set on the window, else it is
declared again. A HUD renders while it is open, ten times a second and at once after the player
acts on it, and the player's acts reach the render as ImGui reports them: a button's call answers
true, a checkbox's answers true and its new value, a text input's, a number's and a slider's the
same, a combo's its new index, a tab bar shows the page chosen. A HUD starts closed (the plugin
bar opens it) unless the script sets `hud.Visible = true`.

| ImGui | declared as |
|---|---|
| `Text`, `TextColored`, `TextWrapped`, `TextDisabled`, `BulletText`, `LabelText`, `Value` | a label (colored where it says) |
| `Button`, `SmallButton`, `ArrowButton`, `Selectable`, `MenuItem`, `RadioButton` | a button |
| `Checkbox`, `CheckboxFlags`, `CollapsingHeader`, `TreeNode` | a toggle (a header's or a tree node's opens what follows) |
| `InputText`, `InputTextWithHint` | an input |
| `InputTextMultiline` | a text box |
| `InputInt`, `InputFloat`, `DragInt`, `DragFloat` (and their two-, three- and four-number forms) | numbers |
| `SliderInt`, `SliderFloat` | a slider |
| `Combo`, `BeginCombo` … `EndCombo` | a choice (a combo's items are its selectables) |
| `ListBox`, `BeginListBox` … `EndListBox` | a list |
| `BeginTabBar`, `BeginTabItem` | tabs and their pages |
| `BeginTable` … `EndTable`, `Columns` | a list where every cell is text, else rows of cells |
| `ProgressBar` | a meter |
| `Image` (a texture from `GetIconTexture` or `CreateTexture`) | an image (the icon or the picture) |
| `PlotLines`, `PlotHistogram` | a chart |
| `SameLine` | the next item in a row with the last |
| `BeginChild`, `BeginGroup`, `BeginPopup` (once opened), `BeginMenu` | a column (a bordered child, a popup and a menu a group) |
| `SetTooltip`, `BeginTooltip` | the last item's tooltip |
| `Begin` … `End` | a window of the script's own |

The window lays itself out, so ImGui's placing calls (`SetCursorPos`, `SameLine`'s offset,
`PushItemWidth`, the style) change nothing, and its measures answer as for a window of the HUD's
size. A HUD's `SetWindowSize` sizes its window. The mouse and the keys are not a plugin's:
`IsMouseDown`, `IsKeyPressed` and their kind answer false, and `IsItemHovered` answers true for
the item before it, so that its tooltip is shown when the pointer rests on it. A window is
declared, not drawn at pixels: the draw lists (`GetWindowDrawList` …), the demo and debug
windows, docking, drag and drop, the contexts and the settings files are refused.
`Huds.Toaster.Add(text)` shows a toast.

### Other modules

| module | here |
|---|---|
| `acclient` | `Movement` turns (`SetHeading`), runs and walks to a place (`RunStraightTo`, `WalkStraightTo`) and holds a movement (`SetMotion`) through the plugin interface's movements; `Combat` attacks and stops; `Client` measures distances, prints to the chat and quits; `DecalD3D`'s objects are world markers, rings and text in the world ([overlays](overlays.md)), shown while `Visible`; `Coordinates` are map coordinates or a landcell and an offset |
| `filesystem` | `GetScript()` reads the script's own folder; `GetData()` reads and writes the plugin's data folder ([api](api.md#the-stores)); `GetCustom` is refused |
| `http` | `Request(url, body, headers)` is an HTTPS request through `net.fetch`, to the domains a manifest names ([peers](peers.md)) |
| `ubnet` | `PublishToChannel` and `SubscribeToChannel` reach the same script in the player's other clients on the machine ([peers](peers.md)); `RemoteExec` is refused |
| `dats`, `utilitybelt.acmessages`, `implot` | refused: the game's data is `game.data`, the messages `game.Messages`, and plots a chart node |

`game.Messages.Incoming.<name>` and `game.Messages.Outgoing.<name>` are events for each message
of the client's [message stream](messages.md), by its name: the argument's `Name`, `Opcode` and
`Data`, its decoded fields under the stream's own names for them.

## What is answered

The counts are of UtilityBelt.Scripting's public members (properties, methods and events, an
overloaded method once) on the classes a script reaches, and of ImGui's functions; a member
refused raises its reason.

| part | answered | refused |
|---|---:|---:|
| `game`, the world, world objects, the character and what hangs off them (24 classes) | 370 | 136 |
| `game.Actions` (every method) | 46 | 0 |
| ImGui's functions | 291 | 61 |

Answered includes members answered from what the client knows rather than all UtilityBelt tells:
a world object's `Stance` and `CombatMode` for anyone but the character, its creation flags,
cooldowns and access times; a skill's experience; `BusyTracker`'s actions; an event the client
never sees happen (`OnSharedCooldownsChanged`, a trade's `OnFailed`). Refused are, mostly, the
parts of the spell table and of skills the plugin interface does not read, and a vendor's terms.

**The client's actions, by UtilityBelt's interface for clients** (`IClientActionsRaw`, the 49
methods a client implements for UtilityBelt's scripts):

| method | here |
|---|---|
| `GetVitalBase`, `GetVitalCurrent`, `GetVitalMax` | the character's vitals and their maximums as attributes; a fellow's from the fellowship |
| `SelectedObject` | `game.selection()` |
| `Login`, `Logout` | `session.select_character`, `session.logout` |
| `AttributeAddExperience`, `SkillAddExperience`, `VitalAddExperience` | partly: `character.raise_attribute`, `raise_skill`, `raise_vital` raise by one at the raise's own cost, and only when the player asked |
| `SkillAdvance` | refused: the plugin interface does not train skills |
| `AllegianceSwear`, `AllegianceBreak` | refused: the plugin interface has no allegiance actions |
| `ApplyWeenie` | `act.use_on` |
| `AutoWield` | `items.wield` (where the item goes) |
| `CastSpell` | `magic.cast`, after entering magic stance |
| `CastEquippedWandSpell` | `act.use_on` of the wielded caster |
| `SetCombatMode` | `combat.set_mode` |
| `DropObject`, `SplitObject`, `GiveWeenie`, `MoveWeenie`, `InscribeWeenie` | `items.drop`, `items.split`, `items.give`, `items.move` (or `items.merge` onto a stack of its kind), `items.inscribe` |
| `FellowshipCreate`, `FellowshipDisband`, `FellowshipDismiss`, `FellowshipSetLeader`, `FellowshipQuit`, `FellowshipRecruit`, `FellowshipSetOpen` | the `social.fellowship_*` actions |
| `InvokeChatParser` | `chat.command` and `chat.say`, and the other plugins' commands above |
| `Appraise`, `SelectWeenie`, `UseWeenie` | `act.appraise`, `act.select`, `act.use` |
| `SalvagePanelAdd`, `SalvagePanelSalvage` | `items.salvage` of what was added |
| `SetAutorun` | `nav.autorun` |
| `TradeAccept`, `TradeAdd`, `TradeDecline`, `TradeEnd`, `TradeReset` | the `trade.*` actions |
| `VendorAddToBuyList`, `VendorAddToSellList`, `VendorBuyAll`, `VendorSellAll`, `VendorClearBuyList`, `VendorClearSellList` | `vendor.buy` and `vendor.sell` of the lists |
| `SendTellByObjectId` | partly: a tell by name, so to a player only |
| `AcceptNextConfirmationRequest` | refused: the plugin interface does not answer the game's confirmations |

So 41 are answered, 4 partly and 4 refused.
