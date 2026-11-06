# ac1999 dat downloads, world stream and movement

What the client takes from an ac1999 server between the login grant and play, and what it sends
to move. Everything here is reached only through explicit ac1999 selection.

## Dat downloads

A server whose `portal.dat` or `cell.dat` is newer than the iterations the login reported brings
the client's files up to its own before it sends the character list, on queue 7 (`ClientCache`).
Nothing is answered: the character list simply follows the last record.

| Opcode | Body after opcode |
|---|---|
| `F7B8` begin | portal iteration, portal record count, cell iteration, cell record count, portal bytes, cell bytes |
| `F7B7` record | record id, cache, compressed length, inflated length, zlib bytes, padding to four |
| `F7AC` cell record | record id, compressed length, inflated length, zlib bytes, padding to four |
| `F7AB` cell pair | as `F7AC` twice, the second id straight after the first's bytes; a zero first id means none |
| `F7BB` touch list | a count, then that many record ids |
| client `F7A9` | record id, cache: a cell record the client lacks (codec only; not sent yet) |

A record's cache names its file: caches 0 to 2 (landblocks, their information and dungeon
cells) are `cell.dat`'s, every other cache `portal.dat`'s. A record inflates to fewer than four
megabytes.

The download is counted as the client counts it. The begin is sent more than once. The first
arms the count of portal and cell records; a second finishes a download whose records are all in
(an empty one finishes there) and otherwise leaves the last record to finish it. A begin with a
zero iteration arms nothing. A touch list counts as one record; cell records asked for outside a
download do not count. Finishing adopts both iterations.

**The player's files are never written.** The relay holds the whole push and serves it through the
later download messages, and Dereth keeps it in its overlay for the world. The next login reports the
overlay's newest iteration, so the server sends only what is newer. A download cut short adopts
nothing, and the server sends it again.

## World stream

The world-object messages of this wire that have the later client's layout are read by the
later codecs:
`F747` delete (id, instance), `F748` position (id, position pack), `F750` sound, `F754` script
by id and `F755` script by type. `F625` carries the appearance in the early form a create uses,
then the instance and appearance stamps; it is normalized to the later appearance message.
A position pack with no `w` reads as zero on this wire and one on the later wire, so the relay states
it. Motion commands pass as they are: a world on files from before 2015 numbers them as those files
do, and Dereth reads a world's commands in that world's numbering.

`F7B0` orders a message by an object's event stamp: the object, the stamp, then a whole message
of the character-list and game set, opcode first. A stamp at or behind the last one taken for
that object is dropped; otherwise the wrapped message is handled as if it came alone.

## Movement

`Movement_MoveToState` (`0xF61C`) goes out as an ordered game action on queue 5: `0xF7B1`, the
action's order number (one more for each action, from one), then the message: the raw motion
state, where the player is, the instance, server-control, teleport and force-position stamps the
server last sent for the player, and the contact byte. The later client writes the same pack, so the
relay forwards it unchanged.

## Proof

Synthetic codecs and an in-process stand-in cover a download of a record split over many parts ahead
of the character list, the world stream through the later messages, and the movement action's framing.
