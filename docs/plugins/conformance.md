# The conformance suite

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

The suite is what makes "works in Dereth" mean "works in any host". It is a set of test plugins,
each with the scenario it runs in and what it must produce, grouped into profiles. A host runs the
profiles it claims and publishes which it passes.

| profile | covers | cases |
|---|---|---|
| **Core** | loading and refusing, events and their order, chat, commands, timers, the stores and their quota, windows as trees, faults, the sandbox; actions and their promises, gestures and automation, appraisals, loot profiles | [`conformance/core/`](conformance/core/) |
| **UI** | HUDs, pictures, canvases and charts, text inputs, sliders, rich list cells, tooltips, toasts and hotkeys | [`conformance/ui/`](conformance/ui/) |
| **Overlay** | markers of each kind, resolved each frame, removed and expiring; the arrow's turn and distance; shapes and text in the world, and nametags; a journey drawn and hidden; presses on the game's map, heard where they fall | [`conformance/overlay/`](conformance/overlay/) |
| **Nav** | moving to a place, following, walking a VTank route, journeys through portals and their plans, a refused portal, stopping a journey, walking routes round walls and through doors and lines of sight, and the automation gate for movement | [`conformance/nav/`](conformance/nav/) |
| **Pregame** | the character list, entering the world, a refused entry and its retry, a world that denies session control | [`conformance/pregame/`](conformance/pregame/) |
| **Peers** | a message between two clients (run by a scenario with two hosts on one bus) | [`conformance/peers/`](conformance/peers/) |
| **Automation** | combat actions and stopping them, the player-killer and parity rules, a meta's states, portal space and death, watchdogs, expressions, VTank meta files and meta views, the heal and debuff helpers | [`conformance/automation/`](conformance/automation/) |
| **UtilityBelt scripts** | scripts written for UtilityBelt's scripting ([ubscript.md](ubscript.md)): a script folder with no manifest, its reads of the game, its action queue, its events and chat, an ImGui HUD as a declared window, its other modules, what is refused, and a named script active before the world | [`conformance/ubscript/`](conformance/ubscript/) |

Core also holds HTTPS to listed domains, a plugin that names no permission, a published policy
document, the message stream, and objects' changes, properties and queries. The
Automation profile holds UtilityBelt's additions to the expression language.

## A case

A case is a folder: a plugin (`plugin.toml`, Lua files, or a UtilityBelt script's folder) and
`expect.toml`.

```toml
says = "What the case proves, in one sentence."

[scenario]                 # absent: no login; the case runs on the host's frames alone
session = "long-solo-play" # a recorded session (Dereth's are in fixtures/packet-captures/), or
# world = "scripted"       # a scripted world: a character holding the [[object]]s below
character = "+Aldric"      # which character to enter the world with; the first when absent
seconds = 120              # seconds to run once in the world

[[object]]                 # a scripted world's objects (see below)
id = 0x80000011
name = "Gold Ring"
type = "jewelry"           # melee_weapon, missile_weapon, caster, armor, clothing, jewelry, gem,
                           # food, money, misc, container, creature, salvage, or the type's bits
container = "player"       # "player" (the main pack) or an object id; absent: on the ground
value = 900
magical = true             # optional: the item is magical
corpse = true              # optional: a corpse, which a use opens
player = true              # optional: a creature that is another player's character
ints = { 105 = 7 }         # what an appraisal of it tells: integer properties
spells = [2074]            # ... and spells
# int64s, floats, strings, bools, dids = { key = value }   # ... the other property tables
# health = [30, 50]        # a creature's health, current and most, as its appraisal tells it
# worn = true              # worn or wielded by the character, in the slot `locations` names
# locations = 0x01000000   # where it may be worn or wielded (the equip mask)
# spell_id = 1679          # the spell it casts when used (a wand's, a food's)
# useability = 0x000A0004  # how it is used and on what (the game's use flags)
# target_type = "creature" # the item types it may be used on

[host]                     # optional
store_quota = 64           # the bytes a store may hold
# data_folder = true       # the plugin has a data folder of its own, kept for the run
policy = { vendor = "deny" }    # rules for automation domains over a world with no policy
# policy_document = """..."""  # or the world's published policy document

[[step]]                   # things the player does, in time order
after = 30                 # seconds after entering the world (or after loading)
say = "/xp reset"          # typed in the chat bar
# click = "main.reset"     # a click on window "main", node "reset"
# set = "main.auto"        # a change of window "main", node "auto" ...
# value = true             # ... to this value
# click = "dereth.plugins/panel.node"      # a node of the host's own window (the plugin panel)
# answer = "yes"           # the player answers the host's dialog for a file, "yes" or "no", ...
# pick = "a.met"           # ... with this file picked
# teleport = [42.5, 33.5]  # the character spends a second in portal space, then stands there
# die = true               # the character dies (its count of deaths rises)
# move_object = { id = 0x80000021, ns = 42.3, ew = 33.8 }   # a scripted object walks there
# hotkey = "go"            # the player presses the plugin's hotkey node "go"
# map_click = { ns = 42.3, ew = 33.8, button = "right", shift = true }
                           # the player presses the game's map there (left, no modifier, by default)
# select = 0x80000051      # the object is selected, as a click on it in the world selects it
# choose = "main.log"      # the player chooses rows of a list (from 1), or a tab of a tabs node, ...
# rows = [2]               # ... these
# hear = { kind = "tell", from = "Bob", id = 0x50000002, text = "hi" }
                           # someone speaks to the character: a tell (the default), "say" or "emote"
# spawn = [{ id = 0x80000051, name = "Drudge", type = "creature", place = { ns = 42.2, ew = 33.8 } }]
                           # objects come into the scripted world (each as an [[object]])
# remove = 0x80000051      # an object leaves the scripted world

[expect]                   # every key optional; each compared exactly
status = "running"         # running, faulted, suspended, or absent (not loaded)
console = ["..."]          # the plugin's console lines, in order (what it printed)
printed = ["..."]          # the local chat lines the plugins and the host printed, in order
hidden = ["..."]           # the incoming lines hidden, in order
sent = ["..."]             # the messages the plugins' actions sent, in order (below)
ui = """..."""             # every window, as ui.md's text form, after `window <plugin> <id> open=<bool>`
```

A case prints what it observed with `print`, so its expectations are the plugin's own words.

**A scripted world** is a character in the world with a main pack, the objects the case lists,
and a server that answers the client the way a server does, a tenth of a second later: an
appraisal with the object's listed properties, a move into a container with the item's new place,
a use with its acknowledgement (after the contents, when the object is a container), a cast with
its acknowledgement. Anything else it leaves unanswered.

**Sent messages** are written one a line: `appraise <id>`, `use <id>`,
`use_with_target <item> <target>`, `put_item_in_container <item> <container> <slot>`,
`wield <item> <slot>`, `cast <spell> [<target>]`, and for any other message its name; ids in
lower-case hexadecimal with `0x`.

## The driver

A host runs a case through five calls:

1. **load a plugin** from the case's folder (a case grants nothing: every plugin may do what each
   permission covers);
2. **feed the scenario**: replay the recorded session's login and enter the world, or enter the
   scripted world;
3. **advance time**: carry on with the recording at its own pace, running frames between its
   messages and the scripted server's answers;
4. **act on the interface**: type a line, click a node, change a node, at each step's time;
5. **collect** the status, the console, the printed and hidden lines, the messages sent, the
   windows, the markers, the movements and session operations asked for, and where the character
   ended, and compare.

A scripted world gives the character and its objects places (`place = { ns, ew }`); the driver
carries out a movement as the character's body would, simplified: it turns at 180° a second and,
while the run lock is on, runs straight ahead at 4 metres a second. A case's `[[wall]]`s (each
`from` and `to`, `[ns, ew]`) stand on the ground: one stops the character, blocks a line of sight
across it, and a walking route goes round its ends; a closed door blocks a line of sight too. A scripted pre-game
(`[scenario.pregame]`) is an account's character list; entering the world as one of them enters
the scripted world as that character, and a character listed `still_in_world` is refused once as
the server refuses it. `[net]` answers HTTPS requests from files in the case's folder; `[host]`
may give the world's published `policy_document` or the player's `policy_rules`. The scripted world
has a small spell table (a few self buffs, a buff for another, a debuff and a bolt), so casting
is tested with no game data.

Dereth's driver replays the recordings through its own session and game model with no game data
files and no display (`dereth-testkit`, `plugins` and `conformance`), stepping the host 30 times a
second; what the plugins ask for goes through the client's own interaction step, as a click in the
interface would. Every Core and UI case runs in its `cpu` test tier.

A case's outcome never turns on how fast the machine runs it. Dereth's driver gives each entry
into a plugin's VM ten seconds, far past what any case's plugin takes, and counts no time against a
plugin's share of a frame; only the case about the time budget itself, `core/budget`, runs at the
host's own budget of 4 ms an entry. Another host's driver should do the same.
