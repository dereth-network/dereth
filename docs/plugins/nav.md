# Movement and routes

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A plugin never holds a key. It asks for a movement (`nav.moveto` capability) and the host steers
the character as a player at the keyboard would. The server sees ordinary movement.

- A **move** (`nav.move_to`) goes straight at one target: the host turns the character in place
  toward it with the client's own turn-to-heading movement, then sets the run lock, stops to turn
  again when the target is more than 15° off (unless it will pass within `range` of it as it
  runs, and so arrive), and lets the run lock go on arrival. It never turns the
  character while running: the client's turn-to-heading ends forward motion when it finishes.
- A **move along** (`nav.move_along`) runs through a list of the plugin's own places with the
  forward key, steering between them while it runs, and stops only to turn at a sharp corner.
  It is what a [route](#routes-derethnav) is walked with.
- A **follow** (`nav.follow`) keeps up with an object that moves: it runs by the run lock and
  steers as a move along does, turning while it runs, aims where the object is going, and walks
  to keep pace near its range ([following](#following)).
- A [journey](#journeys) finds its own way, through portals and recalls too.
- A [path](#paths-and-lines-of-sight) is the client's walking route to a target, answered without
  moving, with whether a straight line to it is clear.

## The player and the server come first

- **The player's own movement** (a movement key or button pressed or released, the run lock, a
  jump, under any interface: a pad's stick and the mouse's run included) ends a plugin's movement
  at once: it fails `interrupted`. The run lock is then the
  player's; the host does not let go of it for them. For 5 seconds after the player's own
  movement, a movement no gesture asked for (`nav.move_to`, `nav.move_along`, `nav.follow`, `nav.face`,
  `nav.autorun(true)`) is refused as `player_moving`; one the player asked for with a gesture is
  not.
- **The server taking control of the character's movement** (to walk it to something used or
  attacked, a stance change, a teleport) ends a plugin's movement too (`interrupted`), and the run
  lock the host set is let go, so nothing of a plugin's fights the server's approach.
- **The run lock is let go only by whoever set it.** The host remembers which plugin set the run
  lock (for a movement, or with `nav.autorun(true)`). A movement's end, `nav.stop()` and
  `nav.autorun(false)` let go of the run lock that plugin set, and nothing else: a run lock the
  player set is never touched, except as the game's own forward key turns any run lock off
  ([what keeps the run lock](#what-keeps-the-run-lock-and-what-lets-it-go)).
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
| `nav.move_along(points [, { range, on_point }])` | having come within `range` metres on the ground (default 2) of each point in turn; answers `{ points, distance }` (how many, and how far from the last) | `stuck` (no nearer the next point for 6 s), `door_shut: …` (as a journey's: see below), `no_position`, `interrupted`, `stopped`, `replaced` |
| `nav.follow(object, range)` | never: it keeps within about `range` (default 3) of the object, as [following](#following) says | `lost` (the object out of sight for 3 s), `interrupted`, `stopped`, `replaced` |
| `nav.face(heading)` | facing within 5° of the heading | `timeout` (3 s), `interrupted`, `stopped`, `replaced` |
| `nav.jump{ ms, heading, walk, direction }` | a frame after the leap; answers `{ held, heading }` (`held`: the milliseconds charged) | `timeout` (3 s to face the heading), `interrupted`, `stopped`, `replaced` |
| `nav.motion(kind, ms [, { walk }])` | once the control has been held `ms`; answers `{ held }` | `interrupted`, `stopped`, `replaced` |
| `nav.autorun(on)` | at once | |

`target` is an object (an id or a handle) or a place, and `points` a list of places. `nav.stop()`
ends the plugin's own running movement (it fails `stopped`) and lets go of the run lock the plugin
set; given a movement's promise (`nav.stop(p)`), only if that is the one running. `nav.owner()`
names the plugin moving the character.

### Moving along a path

`nav.move_along(points, { range, on_point })` is one movement through 1 to 4,096 places, in their
order. It needs the `nav.journey` capability besides `nav.moveto`, since it steers as a journey's
walk does:

- **It walks the points as given, strictly.** It runs along each leg at its point and turns for
  the next only once there (within 0.6 m, or `range` where that is less), however wide `range` is:
  no corner is cut, and nothing is planned round. `range` is how far off a point may be and still
  count: one passed within it (up to 2.5 m, as near as the character was going to come), or one
  it has pressed on toward for half a second from within it and got no nearer (something stands
  on it). A range under 0.25 m counts as 0.25. Heights are not looked at.
- **It runs with the forward key,** never the run lock, so the game says nothing of AutoRun. It
  holds the forward key from point to point and steers while running: a bend inside 20° (the dead
  band of the game's own move-to) is eased through, and a larger one turned with a turn key held
  until it is all but done. It lets the forward key go to turn at a corner sharper than 45°, and
  at a bend too tight to turn in at a run and still come within `range` of the next point.
- **Each point reached is told,** in order: `on_point(i, point)`, or the event
  `nav.point_reached` `{ id, index }` (`id` the promise's, `index` from 1). The last point's
  comes before the promise completes. The character runs on while a handler works; end the
  movement with `nav.stop(p)` to stand at a point.
- **It opens a shut door on its way,** as a journey's walk does. Within 1.5 m of a closed door
  standing across the leg ahead (within a metre of it), the character stands and uses the door,
  again after 4 seconds while it stays shut, and runs on once it stands open. A door the server
  answers as locked, or one still shut after four uses, ends the move:
  `door_shut: <door> is locked, the server says` or `door_shut: <door> did not open after 4 uses`,
  and the failure carries the door as a journey's does (`p.door`: see
  [A shut door](#a-shut-door)). It never picks a lock or looks for a lever.

### Following

`nav.follow(object, range)` keeps the character within about `range` metres of an object (a
player, a creature) until it is stopped, the way a player following someone at the keyboard
would:

- **It turns while it runs.** It runs with the run lock and steers as a move along does: a bend
  inside 20° is eased through, a larger one turned with a turn key held, and it lets the run lock
  go to turn standing only where the way turns more than 45°, as it does when the object comes
  back past it. It never uses the turn-to-heading a move does.
- **It aims where the object is going.** The object's place reaches the client in updates; the
  host reckons its pace from the places it changed to, and aims a moment ahead of it (half a
  second at its pace, 6 m at most) rather than at where it was last seen. An update that comes
  late is taken as the object stopping.
- **It keeps pace near the range.** While the object moves away, the character walks (holding the
  walk key) from the range out to a metre past it, and runs again beyond, so a faster character
  settles at the object's pace without stopping. It stands only within the range of an object
  that is not moving away (a metre past the range before it sets off again), or within half the
  range whatever the object does.
- **It goes round the corner the object turned.** While the line to the object is clear
  (`nav.sight`), it heads straight for it. While something stands between, it follows the
  places the object went through, a step of 2 m apart, from the last place it saw the object at,
  passing each within 1.5 m. Where the host cannot tell whether the line is clear, it follows the
  places too. An object carried more than 40 m at once (a portal, a recall) is followed from
  where it is now.

### What keeps the run lock, and what lets it go

- **Kept:** a plugin's `nav.move_to` or `nav.follow` that replaces its own running one, and one it
  starts within two frames (and a tenth of a second at most, though never less than the one frame
  it hears of the arrival in) of its last move's arrival. Chained moves so keep running, and
  still stop to turn where the next target is more than 15° off and not passed within range.
- **Let go:** a move's arrival with no such move after it; a move's failure; `nav.stop()`; the
  player's or the server's taking control (the player's leaves the lock theirs); the plugin's
  unloading or suspension; another plugin's movement; and any other movement of the plugin's
  (`nav.face`, `nav.jump`, `nav.motion`, `nav.move_along`, a journey) replacing or following the
  move. A follow lets it go while it stands within range, and any move while it turns in place.
- **Never set:** by `nav.move_along` and journeys, which hold the forward key. Pressing forward
  turns a run lock off, the player's too, as the game's own key does. A move along's forward key
  is held on after its arrival as the run lock is, for the plugin's next `nav.move_along` to run
  on with; anything else lets it go.
- **An action waits for it:** a use, a cast or any other action asked for while an arrival still
  holds either lets it go first and is made a frame later, standing.

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
interface to show it ends the journey `declined`. A plugin's journey asks no one: it hands a token
back only when its `opts` say `hand_backs = true` (the plugin has the player's word for it, a
setting of its own, say); otherwise it ends at once, failing `needs the player to agree to leave
the Training Academy`, rather than waiting on a question nobody is there to answer.

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
| `hand_backs` | `false` | `true` lets it hand a token back (leaving the Training Academy for good) without asking |
| `on_planned`, `on_leg`, `on_learned` | | called with this journey's `nav.journey_planned`, `nav.journey_leg` and `nav.journey_learned` events |

The promise completes on arrival with `{ distance, portals, recalls, replans, walked, seconds }` and
fails `no_path` (nothing known reaches the target), `no_target`, `no_route` (a walk found no way),
`door_shut: <door> is locked, the server says` or `door_shut: <door> did not open after 4 uses` (a
door on the way would not open and no walk goes round it; the last `door` leg names it too, and the
failure carries the door: see [A shut door](#a-shut-door)),
`refused: …` (a portal on the only way refuses the character, saying why: a level it needs, a quest
it asks, or what the server said when it refused it before), `only_by_drop: …` (only a drop the body
would not survive leads on), `drop_blocked: …` (the body could not leave the ledge of a drop),
`drop_failed: …`, `stuck`, `too_many_replans`, `interrupted` (the player moved the character),
`dead` (the character died; nothing moves the body until the server stands it up),
`stopped` (`nav.stop()`),
`replaced`, `declined` (the player said no to leaving the Academy), `needs the player to agree to
leave the Training Academy` (it would hand a token back, and `hand_backs` was not given), or at a
hand-back `no_token`
(the person gave no token), `not_taken` (they would not take it back) or `not_sent_on`. A journey
is a movement: one runs at a time across all plugins, and starting any movement replaces it.

A journey's progress arrives as events, to the plugin whose journey it is:

- `nav.journey_planned` `{ id, legs, seconds, reason }`, at the start and each time it is planned
  again (`reason` says why);
- `nav.journey_leg` `{ id, index, kind, to, … }` as each leg begins; `kind` is `walk` (with its
  `path` once its route is planned), `door` (`object`, `name`, and `resist`, its lock's resistance to
  picking once the door has been appraised), `portal` (`name`, `class`, `from`,
  `source`: `learned`, `published` or `hint`), `recall` (`name`, `recall`, `item`), `hand_back`
  (`name`, the person; `token`; `from`, where they stand) or `drop` (`from`, the ledge; `height`,
  metres: a hop off a ledge, taken one way and only where nothing else reaches the target);
- `nav.journey_learned` `{ id, kind, … }`: `portal` (`name`, `to`), `refusal` (`name`, `reason`) or
  `recall` (`recall`, `to`).

### A shut door

The Navigator finds a way and never spends a key or a lockpick, or starts a fight, to open a door:
a journey a shut door stops fails, and its promise says which door and why. `p.door` (also the
second argument of the promise's failure callback, and the `door` of its `action.failed`) is
`{ object, name, why, resist }`: `why` is `locked` (the server answered a use of it as locked),
`no_answer` (used and used again, it did not open), `no_lever` (no walk reaches a lever that works
it) or `lever_failed` (its lever was worked, and it did not open); `resist` is its lock's resistance
to picking once the door has been appraised (`act.appraise`), else nil. A plugin opens it its own
way (`act.use_on` a key or a lockpick on it, `act.use` it, watching `world:get(id).open`) and asks
for the journey again: with the door open it goes through.

A meta's rule at a locked door: use a key the character carries, else pick the lock when the
skill beats its resistance, else stop.

```lua
local function through(target)
	local p = nav.path_to(target)
	local ok = pawait(p)
	if ok or p.door == nil or p.door.why ~= "locked" then
		return ok
	end
	local door = p.door.object
	local a = await(act.appraise(door))
	local resist = a.ints[173]
	local key, pick
	for _, it in ipairs(game.inventory.items()) do
		if it.class == "Key" then
			key = key or it
		elseif it.class == "Lockpick" then
			pick = pick or it
		end
	end
	local skill
	for _, sk in ipairs(game.player.skills or {}) do
		if sk.name == "Lockpick" and sk.training ~= "untrained" then
			skill = sk.current
		end
	end
	if key ~= nil then
		pawait(act.use_on(key.id, door))
	elseif pick ~= nil and skill ~= nil and resist ~= nil and skill > resist then
		pawait(act.use_on(pick.id, door))
	else
		print("stopped at " .. p.door.name .. ": no key, and no pick that beats its lock")
		return false
	end
	pawait(act.use(door))
	for _ = 1, 25 do
		if game.world:get(door).open then
			break
		end
		task.sleep(200)
	end
	return (pawait(nav.path_to(target)))
end
```

`nav.plan(target, opts)` answers the same journey without moving: a promise of `{ legs, seconds }`.
`nav.journey()` reads the plugin's journey under way: `{ legs, leg, target, route, way, goal }`, `leg`
the one being carried out (from 1), `target` where it goes, `route` the way still to go (the walk's
corners, then each later leg's end, a portal's far side among them), and `way` the way still to walk
as one line: from the walk's corner passed last on through each walk to where the route next leaves
the ground, in steps of at most 10 metres (a long straight stretch has places put in along it), and
while the walk is planned again, the way it was following until the new one comes. Walking on along
it only drops its corners from the front; a new plan replaces it. It is what to draw a trail along
(`draw.path`).

While a journey runs the host draws it for the plugin (needing the overlay capabilities): an arrow to
where the leg goes, the way still to go as a line on the map, and each portal and recall ahead as a
point on the map and a blip on the radar with its icon. With `overlay.world` it also draws the way
ahead in the world: a faint wisp that leaves the character at the middle of its body and flows at
that height along the route, held over the ground and floors, turning round the walk's corners on
a curve and on through the walks after it to where the route next leaves the ground (a portal, a
recall) and at most 30 metres, its strands held where they are on the ground and drifting slowly
along it as the character walks past them, thinning and fading as it goes and
fading out where it ends, with a few faint motes drifting off it where it leaves the body. A new
plan or a new journey brings a new wisp in over half a second as the old one goes. Once the
journey's end is within 40 metres, the selection's marker is there, a ring on the ground and an
arrow over it. A plugin that draws a path of its own (`draw.path`, [overlays](overlays.md#paths))
draws the way itself: the host's wisp steps aside while it has one.
`nav.show_journey(false)` hides it and
`nav.show_journey(true)` shows it again; `show = false` starts it hidden.

### What the store knows

`nav.known_portals()` lists this world's portals as the store knows them: `{ name, class, from, to,
source, uses, failures, min_level, max_level, verified, retired }`.

Both lists take a filter, which the client applies itself, so a world of thousands of places costs
a plugin no more than the few it asks for: `{ name, kind, near, within, limit }` keeps those whose
name holds `name` (without case), of `kind` (`nav.known_places` only: one kind or a list), within
`within` meters of `near` (a place; the character when `within` or `limit` is given without it),
nearest first when measured, at most `limit` of them. `nav.known_places{ kind = "town", limit = 5 }`
is the five towns nearest the character.

`nav.search(text, opts)` is the one call for a search as the player types: the portals and places
both, whose names have a word beginning with each word of `text` (`"drudge hid"` finds the Drudge
Hideout; every one when `text` is empty), nearest the character first. It answers `{ total, items }`:
the nearest `limit` (8 unless `opts` says) of the `total` that match, each as the two lists give
it with `list`, `portal` or `place`. `opts` takes `kind` (of places), `portals` and `places`
(`false` leaves that list out), `near` and `within`. The client keeps an index of the lists, so a
search costs the few it answers, however many the world has, and a search that goes on from the
last (a key more) looks only at what that one found.

`nav.recalls()` lists where the
character's recalls have landed: `{ recall, to, uses, verified }`. The store is the client's, kept per
world (the world's name and the server's address), and holds three kinds of knowledge, most trusted
first: what the character learned by using a portal, what the server publishes, and hints. A journey
prefers what was learned, tries a hint only when it is clearly worth it, and never routes through a
portal the character was refused at until a use of it succeeds.

A portal the player uses by hand is learned the same way as one a journey uses. The store learns
where it sent the character, but only when the teleport began at that portal within 20 seconds
of the use. The server moving the character teaches nothing.

The character's recalls are learned for it, kept with the store across sessions:
- **A recall used,** by a journey or by hand (the lifestone, marketplace, house, mansion and
  hometown commands, a recall spell cast, an item that casts one used): where it landed the
  character, once a teleport within 30 seconds of the use has taken the body more than 15 metres.
- **Portal Recall:** where the last portal the character took sent it, at each portal.
- **A tie's recall** (Primary and Secondary Portal Recall, Lifestone Recall) is learned like any
  other the first time it is used.
- **A portal tie:** the tie's recall lands where the portal tied to sends, once the server says the
  tie took and the store knows that portal's destination; **Lifestone Tie** sets Lifestone Recall
  to land at the lifestone tied to.
- **A lifestone used:** the lifestone command lands where the character stood when the lifestone
  took its spirit.
- **The Marketplace recall** lands every character at the Marketplace, where this world's places
  name it, until a use says otherwise.
- **What the world tells the client of the character:** where it keeps the lifestone the character
  last used, the lifestone it tied to and the last portal it took, where a world sends them.

A journey plans with a recall only once its landing is known; it never tries one to find out.

### The places it has learned

`nav.known_places()` lists the places the client knows of this world: `{ kind, name, class, source,
at, seen, owner }`. It is the same set the client's own map draws.
- `kind` is `town`, `landmark`, `cottage`, `villa` or `mansion` for a place the game's data names
  (`source` `data`: read from the player's own files, the towns and landmarks the game's map
  names and every house by its building), and `lifestone`, `portal`, `vendor`, `npc` (a person who
  is neither a vendor nor attackable) or `house` for one the client learned.
- `at` is where the place stands, as `game.player.position` gives places.
- `seen` is when it was last seen, in seconds since 1970.
- `owner` is a house's owner; a house without one is for sale.

Each place is learned when the server puts it in the world in front of the character. A house is
learned when the player uses its crystal, along with the owner the server names. Places are kept
per world, as the portals are, and never include a player or anything carried. A place is not
forgotten for being out of sight: seeing it again updates it.

A world whose server publishes its places adds them too, with `source` `published` and `seen` 0
until the player sees them. What the player saw always wins over what the server says.

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
walked with `nav.move_to` corner by corner, or `nav.move_along` through the corners up to the next
door, each door in `doors` used (`act.use`) when its corner is reached.

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

Each stretch of plain points runs as one `nav.move_along` (on a host with `nav.journey`; one
`nav.move_to` a point otherwise), so the character runs through them without stopping, each come
within `range` (3 m unless given) and no corner cut, round a circular route's wrap too. It stops
at every other kind of point, standing for what is done there, and at the route's end. Where the
next point is out of sight it breaks off for the client's walking route there (unless `straight`).
`on_point` hears each point reached; where it waits (a `task.sleep`, an `await`), the character
stops and waits for it.
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
