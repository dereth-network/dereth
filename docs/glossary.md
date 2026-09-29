# Glossary

**Provenance: mixed.** The game's own vocabulary and the wire/file vocabulary are format and
observation; the few rebuild terms are marked *(rebuild)* and are design opinion.

Asheron's Call carries twenty-five years of jargon, and a reader meets it all at once. This page
defines each term once and points at the page that treats it properly. Where the game's usage of a
word differs from ordinary industry usage — and it often does — the entry says so.

## If you only learn nine terms

1. **weenie** — a world object. Everything you can see, hold or kill is a weenie.
2. **DataID** (DID) — a 32-bit id of a *file inside a dat*; the top byte selects the file type.
3. **ObjectID** (IID) — a 32-bit id of a *live world object*. ACE calls it a GUID.
4. **dat** — one of the four container files that hold every asset and table.
5. **landblock** — a 192 m square of world; its id is the top 16 bits of every cell id inside it.
6. **cell** — the unit of space occupancy: land cells outdoors, environment cells indoors.
7. **setup** — a multi-part model definition; a part array instantiates it into parts.
8. **blob** — one whole protocol message, fragmented for transmission and reassembled on arrival.
9. **quality** — a typed property of a world object. The whole of gameplay state is a bag of them.

## A

| Term | Meaning | See |
|---|---|---|
| **ACE / ACEmulator** | The open-source C# server emulator started after the 2017 shutdown. It ports the client's physics closely, parses every dat type, and carries every enum, which makes it the best secondary source that exists — and where it disagrees with the client, the client is what the client does. | [CORRECTIONS.md](CORRECTIONS.md) |
| **ACViewer** | A C#/MonoGame viewer for the dat files — models, textures, landblocks, particles. Useful as an independent visual read of a dat interpretation. | — |
| **allegiance** | The game's pyramid of sworn players: each character has one patron and any number of vassals, and the root of a tree is its monarch. The client keeps the tree and draws the panel; the server owns the rules. | — |
| **animation hook** | A timed side effect attached to a frame of an animation: play a sound, start an emitter, swap a part's texture, replace an object. There are 27 hook types, each with its own payload. | — |
| **appraisal profile** | The block of information the server returns when a player assesses an object: values, highlight bitfields, and per-class extensions for creatures, weapons, armour, hooks and houses. | — |
| **archive** | The newer of the two serialisation streams: a cursor over a buffer with alignment rules, a version stack and a flag set. Most gameplay tables and every cached dat object are written in it. Distinct from the older *pack* form. | — |
| **autonomy** | Whether a movement was decided by the client for its own object (autonomous) or dictated by the server. Autonomous movements are reported upward with their own position; server-driven ones are applied on arrival. | — |

## B

| Term | Meaning | See |
|---|---|---|
| **blob** | One complete protocol message plus its routing metadata (queue, priority, ordering stamps) and a 64-bit id. Blobs are cut into fragments for transmission and reassembled before dispatch. | [networking/04-netblobs-and-queues.md](networking/04-netblobs-and-queues.md) |
| **BSP tree** | The binary space partition stored inside a graphics object and inside cell geometry. There are separate trees for drawing, for physics and for cell-portal traversal; collision tests descend the physics one. | — |

## C

| Term | Meaning | See |
|---|---|---|
| **cell** | The unit of space occupancy and the key to visibility. Outdoors a cell is one 24 m square of a landblock; indoors it is one room-shaped environment cell. A cell id whose low 16 bits are `0x0001`–`0x0040` is outdoors, and `0x0100` or above is indoors; the indices between, and zero, are not cells. | — |
| **cell struct** | One interior cell *shape* inside an environment file: its geometry, its polygons and its portal polygons. An environment cell is a cell struct placed with a frame. | — |
| **clothing base** | An entry in a clothing table, keyed by setup id, that says "on part *n* use this graphics object, this texture and this palette range". It is how one body model wears many outfits. | — |
| **collision profile** | The small record a physics object hands to a collision report describing what it is — player, missile, creature, door, cloaked, ethereal — so the receiver can decide what the collision means. | — |

## D

| Term | Meaning | See |
|---|---|---|
| **dat** | One of the four container files the client reads: the portal dat, the cell dat, a per-language dat and a high-resolution texture dat. Each is a block-chained file with an order-62 B-tree directory and optional per-file compression. | — |
| **DataID (DID)** | A 32-bit id of a file inside a dat. The high byte selects the type, which in turn selects which dat file holds it and which decoder unpacks it. | — |
| **DDD** | The client's in-game dat patch protocol ("data distribution"), run as its own startup phase: the server declares which dat files and iterations it expects, the client asks for what it is missing, and writes what arrives into its dats. | [networking/03-connection-state-machine.md](networking/03-connection-state-machine.md) |
| **Decal** | The long-lived third-party plugin framework injected into the original client to draw overlays and script play. Context only; the client's own plugin surface is separate and was barely used. | — |
| **degrade** | The adaptive level-of-detail system: a global multiplier lowers model detail, particle counts and texture levels when the frame rate drops. Per-object detail chains come from a degrade-info file. | — |
| **descriptor heap** | *(rebuild)* A Direct3D 12 array of resource views that shaders index into. The original has no equivalent — it binds one texture per stage through cached setters — so descriptor tables are a new construct rather than a translation of an old one. | — |

## E

| Term | Meaning | See |
|---|---|---|
| **element description** | The description of one UI element in a layout: its type, its rectangle, its anchoring, its attributes and its per-state appearance. Descriptions inherit from one another before a live element tree is built. | — |
| **enchantment registry** | The per-object table of active spell effects, with the rules that decide which of two competing enchantments wins, the culling of expired entries, and the order their modifiers apply to a quality. | — |
| **environment cell** | An interior cell: a placed instance of a cell struct from an environment file, with its own portals, static objects and lighting. Dungeons and building interiors are made of them. | — |
| **ephemeral** | A property of a blob id: an ephemeral blob belongs to a stream in which only the newest matters, so an older one may be discarded on arrival. | [networking/04-netblobs-and-queues.md](networking/04-netblobs-and-queues.md) |
| **ethereal** | A physics object that collides with nothing. It is a state bit, and it suppresses collision *reports* rather than merely passing the tests. | — |

## F

| Term | Meaning | See |
|---|---|---|
| **fellowship** | A temporary party of up to nine players that shares experience and loot. The client holds the roster and the share settings and formats the deltas the server sends. | [CORRECTIONS.md](CORRECTIONS.md) |
| **fixed-function state** | *(rebuild)* The original has no shaders. A material is a set of render-state, texture-stage and sampler values applied before a draw, so reproducing the renderer means turning each distinct combination into a pipeline state object plus a few constants. | — |
| **fragment** | The unit a blob is cut into so it fits in a datagram: a 16-byte fragment header plus payload, at most 464 bytes whole. Fragments of one blob are reassembled per connection. | [networking/01-packet-format.md](networking/01-packet-format.md) |
| **framework** | The object that owns one whole screen — intro, character select, gameplay and so on. Exactly one exists at a time, and switching UI mode destroys the old one before constructing the new. | — |

## G

| Term | Meaning | See |
|---|---|---|
| **GDLE / GDLEnhanced** | A C++ server emulator that aims to match the original server's behaviour. A useful second opinion where ACE is silent or has diverged. | [CORRECTIONS.md](CORRECTIONS.md) |
| **graphics object** | The leaf of every model (DataID type `0x01`): vertices, polygons, BSP trees, and the drawing and physics geometry of one rigid piece. | — |
| **GLS ticket** | One of the three authentication forms the login request can carry. The launcher obtains a ticket from the account directory service and the client presents it instead of a password. | [networking/03-connection-state-machine.md](networking/03-connection-state-machine.md) |
| **GUID** | ACE's name for an ObjectID. | — |

## I

| Term | Meaning | See |
|---|---|---|
| **ICMD** | The out-of-band command section of the transport. Its only live use is a periodic no-op sent to the server's second port to keep a NAT binding alive. | [networking/01-packet-format.md](networking/01-packet-format.md) |
| **IID** | Interchangeable with ObjectID: a 32-bit live-object id. It is also the name of the quality type whose values are object ids. | — |
| **image texture** | A texture *reference* object (DataID type `0x05`): it names a surface bitmap plus the palette and colour key to apply to it. The bitmap itself is a separate render-surface file. | — |
| **iteration** | Two unrelated senses. (a) A dat file's version counter: the server declares the iteration it expects and a client behind it must run DDD to catch up. (b) A field in the transport header naming the connection's generation, so that a re-handshake can replace a stale connection. | [networking/01-packet-format.md](networking/01-packet-format.md) |

## K

| Term | Meaning | See |
|---|---|---|
| **KSML** | The markup language of the embedded browser component the original used for the help panel and the plugin panels. | — |

## L

| Term | Meaning | See |
|---|---|---|
| **land cell** | An outdoor cell: one 24 m square of a landblock, holding two terrain triangles and the objects standing on them. | — |
| **landblock** | A 192 m square of the world, 8 × 8 land cells of 24 m. The world is a 255 × 255 grid of them; a landblock id forms the high 16 bits of every cell id inside it. | — |
| **land height table** | The global table that turns a terrain height *byte* into metres. It is not linear: doubling the byte is right only for the low part of the range, and the table's top entries reach 700.0 m. | [CORRECTIONS.md](CORRECTIONS.md) |
| **layout** | A whole UI screen or panel description loaded from the language dat: a tree of element descriptions plus the states they can be in. | — |

## M

| Term | Meaning | See |
|---|---|---|
| **monarch** | The character at the root of an allegiance tree. | — |
| **motion command** | One of the 408 named actions a creature can be in or perform — ready, walk forward, attack high, and so on. Combined with a stance it keys the motion table. | — |
| **motion table** | The dat object (type `0x09`) that maps *(stance, from-motion, to-motion)* to a sequence of animations, with cycles, links and per-motion speed scaling. | — |
| **move-to** | A movement request expressed as a destination rather than a key press: walk to this object, walk to this position, turn to this heading. A small state machine re-issues motion commands until the goal is reached. | — |

## N

| Term | Meaning | See |
|---|---|---|
| **notice** | The client's internal broadcast event: a numbered notification — character set arrived, server died, object selected — delivered to every registered handler. It is the main decoupling mechanism between subsystems and the UI. | — |

## O

| Term | Meaning | See |
|---|---|---|
| **ObjectID** | A 32-bit id of a live world object. Ranges partition players, dynamic objects and static objects. | — |
| **optional header** | One of the 19 variable sections that may follow a datagram's 20-byte transport header — acknowledgements, retransmit requests, flow accounting, time sync, echo, login data and the rest. Their presence is announced by flag bits and their order on the wire is fixed. | [networking/01-packet-format.md](networking/01-packet-format.md) |
| **ordering queue** | The property that messages on the same queue reach their handler in send order, enforced by a per-queue stamp. Different queues are independent of one another. | [networking/04-netblobs-and-queues.md](networking/04-netblobs-and-queues.md) |

## P

| Term | Meaning | See |
|---|---|---|
| **packed integer** | A variable-length integer used by the archive format: one byte below `0x80`, two below `0xC0`, otherwise four. Counts and string lengths use it. ACE calls the same thing a compressed uint. | — |
| **pack form** | The older serialisation interface — ask for the size, write into a raw buffer, read back out of one. It coexists with the archive format; network message bodies and a few dat types use it. | — |
| **palette set** | A dat object (type `0x0F`) holding a list of palettes to choose from, so that one model can be recoloured per instance. | — |
| **part array** | The runtime instantiation of a setup: the array of parts, their world frames, their materials, and the motion table wired up to animate them. | — |
| **patron** | The character one rung above you in an allegiance; you are their vassal. | — |
| **physics script** | A short timeline of effects applied to an object — particle bursts, sounds, part swaps, translucency ramps — used for spell effects, damage flashes and portals. | — |
| **portal** | A polygon connecting two cells, used both for visibility traversal and for moving an object between cells. Distinct from an in-game teleport portal, which is an ordinary weenie. | — |
| **script type** | The semantic name of a physics-script effect, for example a red health flash. The server sends these rather than DataIDs; a table resolves a type plus an intensity to a concrete script file. | — |
| **PSO** | *(rebuild)* A Direct3D 12 pipeline state object: the immutable bundle of blend, depth, raster and shader state bound in one call. Fifteen of them cover the original's whole legacy surface path. | — |

## Q

| Term | Meaning | See |
|---|---|---|
| **quality** | A typed property of a world object — int, int64, bool, float, string, DataID, IID, attribute, secondary attribute or position. | — |
| **queue** *(networking)* | One of the twelve numbered channels a blob can travel on. Each has its own ordering, its own dispatcher and its own draining subsystem. | [networking/04-netblobs-and-queues.md](networking/04-netblobs-and-queues.md) |

## R

| Term | Meaning | See |
|---|---|---|
| **region** | The single dat object describing the world's landscape rules: the land height table, terrain types and their textures, scene lists, sky, lighting, weather and ambient sound tables. | — |
| **root signature** | *(rebuild)* The Direct3D 12 declaration of what a pipeline's shaders can see. The original has nothing equivalent. | — |

## S

| Term | Meaning | See |
|---|---|---|
| **scenery** | Trees, bushes and rocks generated procedurally on terrain from a scene list rather than placed by hand. Their positions come from a hash of the cell coordinates, so client and server agree without sending anything — provided both apply the same road and slope filters. | [CORRECTIONS.md](CORRECTIONS.md) |
| **sequence** | A per-connection or per-object counter that orders messages and rejects stale ones. | [networking/02-reliability-and-flow.md](networking/02-reliability-and-flow.md) |
| **setup** | A dat object (type `0x02`) describing a multi-part model: the list of part graphics objects, their placement frames, the parent/child hierarchy, holding locations and default animations. | — |
| **shadow object** | A lightweight stand-in a physics object leaves in each additional cell it overlaps, so that cell lists and drawing see it without duplicating the object. | — |
| **sphere path** | The swept-sphere description of a proposed move: the source and destination positions, the spheres involved, and the accumulated results of the tests along the way. | — |
| **stab list** | The pre-computed set of cell ids visible from a given cell — a potentially visible set. Used to prune cell lists and to prefetch interior cells. | — |
| **stance** | The posture a creature holds: unarmed, sword and shield, bow, magic and so on. It is the outer key of every motion-table lookup, so the same motion command animates differently in different stances. | — |
| **string reference** | A localised string: a string-table id plus a variable list, resolved at display time through the language dat and its metalanguage renderer rather than stored as text. | — |
| **subpalette** | A contiguous range inside a palette that a clothing or object description recolours independently, so one model can have several separately tinted regions. | — |
| **surface** | The dat material a polygon points at (type `0x08`): a texture or a solid colour, plus the flag bits that decide blending, alpha test, depth write and culling. | — |

## T

| Term | Meaning | See |
|---|---|---|
| **terrain merge** | The terrain texture compositor: it takes the terrain types of a cell's corners, chooses a base, and blends the others over it with alpha masks to produce one merged surface per cell. | — |
| **text tag** | An inline markup token inside a displayed string that turns a run of text into a link, an icon or a coloured span, and reports clicks back as a notice. | — |
| **timestamp** | A per-object monotonic counter attached to state updates so that an out-of-order or stale update can be discarded. Physics, movement and quality updates each have their own. | — |
| **transition** | One attempted move of a physics object from a position to a position: build the cell list, sweep the spheres, resolve steps, slides and collisions, and either commit the new position or report why not. | — |

## U

| Term | Meaning | See |
|---|---|---|
| **UI mode** | The client's top-level state: intro, data patch, character select, character creation, gameplay, credits, epilogue, disconnected. Each mode maps to one screen, and a switch is deferred to a single point in the frame. | — |

## V

| Term | Meaning | See |
|---|---|---|
| **vassal** | A character sworn to a patron in an allegiance. | — |
| **vitae** | The experience penalty applied on death, carried as a quality and worked off by earning experience. | — |

## W

| Term | Meaning | See |
|---|---|---|
| **wcid** | Weenie class id: the id of the *template* an object was created from, as opposed to its ObjectID, which identifies the instance. | — |
| **weenie** | A world object: anything that exists in the world and can be seen, held, used or killed. A live one is a physics object, a game-state object and a cell entry travelling together — three things with independent lifetimes, which is why a client routinely holds one without the others. | [networking/messages/02-world-objects.md](networking/messages/02-world-objects.md) |
