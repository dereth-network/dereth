# Movement and routes

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A plugin never holds a key. It asks for a movement (`nav.moveto` capability) and the host steers
the character as a player at the keyboard would: it turns the character in place toward the target
with the client's own turn-to-heading movement, then sets the run lock, stops to turn again when
the target is more than 15° off, and lets the run lock go on arrival. It never turns the character
while running: the client's turn-to-heading ends forward motion when it finishes. The server sees
ordinary movement. A move goes straight at its target, and a route is a list of points walked one
move at a time; a [journey](#journeys) finds its own way, through portals and recalls too; and a
[path](#paths-and-lines-of-sight) is the client's walking route to a target, answered without
moving, with whether a straight line to it is clear.

## The player and the server come first

- **The player's own movement** (a movement key or button pressed or released, the run lock, a
  jump, under any interface: a pad's stick and the mouse's run included) ends a plugin's movement
  at once: it fails `interrupted`. The run lock is then the
  player's; the host does not let go of it for them. For 5 seconds after the player's own
  movement, a movement no gesture asked for (`nav.move_to`, `nav.follow`, `nav.face`,
  `nav.autorun(true)`) is refused as `player_moving`; one the player asked for with a gesture is
  not.
- **The server taking control of the character's movement** (to walk it to something used or
  attacked, a stance change, a teleport) ends a plugin's movement too (`interrupted`), and the run
  lock the host set is let go, so nothing of a plugin's fights the server's approach.
- **The run lock is let go only by whoever set it.** The host remembers which plugin set the run
  lock (for a movement, or with `nav.autorun(true)`). A movement's end, `nav.stop()` and
  `nav.autorun(false)` let go of the run lock that plugin set, and nothing else: a run lock the
  player set is never touched.
- **A plugin unloaded or suspended** mid-movement lets go of the character: its movement stops
  and the run lock it set is let go.

## Places

A place is a table, one of two forms:

- **map coordinates**, `{ ns = 42.2, ew = 33.8 }`, south and west negative, as the radar shows
  them; these name places outdoors. One map unit is 240 metres.
- **cell and offset**, `{ cell = 0x01D90108, x = 10, y = 20, z = 0 }`, which names any place,
  indoors or out.

`game.player.position` and an object handle's `position` answer `{ cell, x, y, z, heading, ns, ew }`
(`ns` and `ew` only outdoors; `heading` in degrees clockwise from north). `object:distance()` is the
distance from the character in metres.

## Movements

Each movement is a promise (see [actions](actions.md)); one runs at a time across all plugins, and
starting another fails the one running with `replaced`.

| call | completes | fails |
|---|---|---|
| `nav.move_to(target, range)` | within `range` metres on the ground (default 2); answers `{ distance }` | `stuck` (no half-metre of progress in 6 s), `timeout` (slower than 2 m/s plus 20 s), `target_gone`, `no_position`, `interrupted`, `stopped`, `replaced` |
| `nav.follow(object, range)` | never: it closes to within `range` (default 3) and holds there | `lost` (the object out of sight for 3 s), `interrupted`, `stopped`, `replaced` |
| `nav.face(heading)` | facing within 5° of the heading | `timeout` (3 s), `interrupted`, `stopped`, `replaced` |
| `nav.jump{ ms, heading, walk, direction }` | a frame after the leap; answers `{ held, heading }` (`held`: the milliseconds charged) | `timeout` (3 s to face the heading), `interrupted`, `stopped`, `replaced` |
| `nav.motion(kind, ms [, { walk }])` | once the control has been held `ms`; answers `{ held }` | `interrupted`, `stopped`, `replaced` |
| `nav.autorun(on)` | at once | |

`target` is an object (an id or a handle) or a place. `nav.stop()` ends the plugin's own running
movement (it fails `stopped`) and lets go of the run lock the plugin set; `nav.owner()` names the plugin moving the character.

**Jumps and held motions** press the game's own movement controls for the plugin, as the game's
own movement under any interface, the way a player holds them:

- `nav.jump{ ms, heading, walk, direction }` turns the character to `heading` first (degrees; left
  out, it jumps the way it faces), then presses the jump and holds it for `ms` (0 to 1000; the
  power bar is full at a second, so a longer charge leaps no farther and is refused), holding the
  walk key while it charges when `walk` is true, and the control of `direction` (`forward`,
  `backward`, `strafe_left` or `strafe_right`; left out, straight up). The jump is pressed before
  the direction, so a jump charged standing still stands while it charges and leaps the way the
  direction says, as a player's standing long jump does. Then it lets go of the jump: the leap.
- `nav.motion(kind, ms, { walk })` holds one control, `forward`, `backward`, `strafe_left`,
  `strafe_right`, `turn_left` or `turn_right`, for `ms` (at most 10,000), with the walk key when
  `walk` is true, then lets go.

Whatever the host holds for a movement it lets go of when the movement ends, however it ends:
finished, failed, replaced, stopped, interrupted by the player or the server, or the plugin's
unloading or suspension. A jump still charging then is put down without a leap, as the Escape key
puts one down.

A movement's domain is `nav`: with a live gesture it needs `act.interactive`, without one
`auto.nav` and a world that allows `nav` without a gesture (see [permissions](permissions.md)). A
follow started by a gesture keeps running after the activation ends. **Stopping is always
allowed:** `nav.stop()` and `nav.autorun(false)` need no gesture, no permission and no leave of the
world (nor does a player killer's character in a world that stops its automation), since each lets
go only of what the plugin itself holds.

## Journeys

`nav.path_to(target, opts)` asks the host for a **journey** (`nav.journey` capability): the host
plans it over walking, the portals this world's store knows and the character's own recalls, and
carries it out. A walk goes round what stands in the way, through doors, and a long way outdoors a
stretch at a time; a portal is walked to and used; a recall is used by its item, by its command, or
cast after taking up a caster and going into the magic stance. What a use shows is learned: where a
portal sent the character, that a portal refused it, where a recall landed. A leg that fails is
learned from and the journey planned again. A journey steers with the game's own movement
whatever an interface makes of the player's keys (under an interface whose camera turns the
movement keys into the camera's directions, its turns are still turns), and the host plans
nothing, and reads nothing of the world for planning, until a journey or a plan is asked for.

**Leaving the Training Academy.** A new character begins in a Training Academy, a dungeon whose
first rooms no portal leaves: its way out is Jonathan, who gives the character an Academy Exit
Token and, handed it back, sends it to its town for good. Where the character stands in an
Academy (a landblock of the character-creation table's starts) and the world shows Jonathan
there, a journey may hand his token back as a leg (`hand_back`): it walks to him, uses him for the
token (again a while later when it does not come), gives it back once he has done talking, and
arrives where the Academy's own exit portal sends (the server's published table, or a use, says
where); a portal out of the same rooms is taken first. Before such a journey moves at all the
player is asked, in the client's own yes-or-no box that every interface shows, whether to leave
the Academy, which cannot be entered again; a no, a box taken down unanswered, or a client with no
interface to show it ends the journey `declined`.

`target` is an object, a place, `{ cell = 0x… }` (the cell's middle), `{ portal = "name" }` (where
that known portal sends) or `{ recall = "lifestone" | "marketplace" | spell }` (where that recall
landed the character last). A journey to an object whose own place no walk reaches (a
shopkeeper behind a counter, someone in a booth with a window onto the street) ends at the place
nearest it that a walk does reach, within 3 metres of it, the distance most people are used from.
`opts`:

| key | default | |
|---|---|---|
| `portals` | `true` | `false` leaves portals out |
| `recalls` | `true` | `false` leaves the character's recalls out |
| `show` | `true` | `false`: the host does not draw it |
| `on_planned`, `on_leg`, `on_learned` | | called with this journey's `nav.journey_planned`, `nav.journey_leg` and `nav.journey_learned` events |

The promise completes on arrival with `{ distance, portals, recalls, replans, walked, seconds }` and
fails `no_path` (nothing known reaches the target), `no_target`, `no_route` (a walk found no way),
`stuck`, `too_many_replans`, `interrupted` (the player moved the character), `stopped` (`nav.stop()`),
`replaced`, `declined` (the player said no to leaving the Academy), or at a hand-back `no_token`
(the person gave no token), `not_taken` (they would not take it back) or `not_sent_on`. A journey
is a movement: one runs at a time across all plugins, and starting any movement replaces it.

A journey's progress arrives as events, to the plugin whose journey it is:

- `nav.journey_planned` `{ id, legs, seconds, reason }`, at the start and each time it is planned
  again (`reason` says why);
- `nav.journey_leg` `{ id, index, kind, to, … }` as each leg begins; `kind` is `walk` (with its
  `path` once its route is planned), `door` (`object`), `portal` (`name`, `class`, `from`,
  `source`: `learned`, `published` or `hint`), `recall` (`name`, `recall`, `item`) or `hand_back`
  (`name`, the person; `token`; `from`, where they stand);
- `nav.journey_learned` `{ id, kind, … }`: `portal` (`name`, `to`), `refusal` (`name`, `reason`) or
  `recall` (`recall`, `to`).

`nav.plan(target, opts)` answers the same journey without moving: a promise of `{ legs, seconds }`.
`nav.journey()` reads the plugin's journey under way: `{ legs, leg, target, route, goal }`, `leg` the
one being carried out (from 1), `target` where it goes, `route` the way still to go.

While a journey runs the host draws it for the plugin (needing the overlay capabilities): an arrow to
where the leg goes, the way still to go as a line on the map, and each portal and recall ahead as a
point on the map and a blip on the radar with its icon. `nav.show_journey(false)` hides it and
`nav.show_journey(true)` shows it again; `show = false` starts it hidden.

### What the store knows

`nav.known_portals()` lists this world's portals as the store knows them: `{ name, class, from, to,
source, uses, failures, min_level, max_level, verified, retired }`. `nav.recalls()` lists where the
character's recalls have landed: `{ recall, to, uses, verified }`. The store is the client's, kept per
world (the world's name and the server's address), and holds three kinds of knowledge, most trusted
first: what the character learned by using a portal, what the server publishes, and hints. A journey
prefers what was learned, tries a hint only when it is clearly worth it, and never routes through a
portal the character was refused at until a use of it succeeds.

## Paths and lines of sight

The client's own planner, the one its journeys walk, answers two questions without moving the
character (`nav.path` capability). Neither is an action of any domain: they move nothing.

`nav.path(target)` is a promise of the **walking route** from where the character stands to
`target` (an object, a place or `{ cell = 0x… }`), round walls and the objects that stop a body,
through doors, and across a dungeon's rooms or into and out of buildings:

| key | value |
|---|---|
| `points` | the corners, places as `game.player.position` gives them: the first where the character stands, the last the place to stand nearest the target (for an object no walk reaches, the nearest place a walk does reach, within 3 metres of it) |
| `doors` | the closed doors on the way, each `{ object, at }`: the door's id, and the corner (counting from 1) it is opened from before the leg after it walks through |
| `length` | metres along the corners |
| `complete` | `false` when the target lies beyond what one plan covers (a long way outdoors): the route ends on the way there, and is asked for again from its end |

It fails `no_route` (no walkable way reaches the target: walled in, up a ledge), `too_far`,
`no_target`, `no_position` or `no_world` (the world is not built yet). A monster stands in the
route's way only where there is no way round it. Each leg of the route is straight, so it is
walked with `nav.move_to` corner by corner, each door in `doors` used (`act.use`) when its corner
is reached.

`nav.sight(target [, from])` answers at once whether a **straight line** from the character (or
`from`, an object or a place) to `target` (an object or a place) is clear, each end a metre above
the ground it stands on: `false` where a wall, the ground, a closed door or another object that
stops a body (a tree, a rock) stands between, what a bolt or an arrow would hit first; creatures
do not block it. It is `nil` where the host cannot tell (no world built).

### Domains

A journey's domain is `nav`, and so are the portals it uses, its hand-backs (the player says yes to
each first), its recall items and the recalls by command; a recall spell is a spell on oneself, the
`buff` domain. A journey with no gesture behind it needs `auto.nav` and a world that allows `nav`
without one; without `auto.buff` (or a world that allows it) its plan leaves the character's recall
spells out.

## Routes: `dereth.nav`

```lua
local nav_lib = require("dereth.nav")
local route = nav_lib.load(files.package("loop.nav"))     -- a VTank route file
local mine = nav_lib.route({ { ns = 42.2, ew = 33.8 }, { ns = 42.3, ew = 33.8 } }, "circular")
task.spawn(function()
  route:walk{ range = 3, on_point = function(i, point) print("reached", i) end }
end)
```

A route is `{ kind, points }`: `kind` is `circular` (starts over), `linear` (turns back at each
end), `once` (ends at its last point) or `follow`; each point is `{ kind, position, … }`. `walk`
moves to each point in turn inside `task.spawn`, sleeps a `pause` point's `ms`, types a `chat`
point's `text`, casts a `recall` point's `spell`, jumps at a `jump` point (`nav.jump` with the
point's `heading`, its `shift` as `walk` and its `ms`), and raises the movement's failure if one
fails.
`route:nearest()` is the index of the point nearest the character.

`nav_lib.load` reads VTank's `.nav` text format:

```text
uTank2 NAV 1.2        the header
1                     the route kind: 1 circular, 2 linear, 3 follow, 4 once
3                     how many waypoints (a follow route has the target's name and id instead)
0                     each waypoint: its type, then east-west, north-south, height (map units),
33.8                  an unused line, then its type's own lines
42.2
0
0
…
```

| type | point kind | its own lines |
|---|---|---|
| 0 | `point` | none |
| 1, 6 | `portal` | the portal's name, its object class, an unused line, the exit's north-south, east-west and height |
| 2 | `recall` | the recall spell's id |
| 3 | `pause` | milliseconds |
| 4 | `chat` | the line to type |
| 5 | `vendor` | the vendor's id, its name |
| 7 | `use_npc` | the NPC's name, its object class, an unused line, its east-west, north-south and height |
| 8 | `checkpoint` | none |
| 9 | `jump` | heading, whether shift is held, milliseconds |

Any other type is an error, since its lines cannot be skipped. `walk` treats portals, vendors,
NPCs and checkpoints as points to walk to; using them is the plugin's to add. A follow route reads
as `{ kind = "follow", target, target_id }` (its player's name and object id), and `walk` follows
that player (`nav.follow`, by its id while the client knows it, else the nearest of its name),
looking for it again a second after it is lost, until stopped.
