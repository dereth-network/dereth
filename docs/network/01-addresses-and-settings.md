# Addresses, settings and stand-in ids

Provenance: **network specification**, shaped by what the final retail client (June 2015) does,
**observed at runtime** where noted.

One implementation: `network/protocol/src/address.rs`, `settings.rs`, `standin.rs` and, for what
the directory publishes, `directory.rs`. The vectors in `network/conformance/vectors/addresses.json` pin
the rules below.

## 1. Addresses

A character on another world is addressed as **`Name@world`**: the character's name, the separator
`@`, and its home world's **network slug** — `Bob@coldeve`.

- **Why `@`.** The retail client's character creation lets a name hold only ASCII letters, spaces,
  apostrophes and hyphens, at most 32 characters, so no retail character's name can hold `@`. A
  typed `@tell Bob@coldeve, hi` reaches the world as `Communication_TalkDirectByName` (`0x005D`)
  with the name `Bob@coldeve` exactly: the client splits the line at the first comma, strips every
  leading `+` and checks nothing else (observed at runtime).
- **A world must not let a local character's name hold `@`**, through creation, renaming or any
  tool, because the separator is how it tells a network address from a local name. (Emulators
  generally do not check name characters on the server, so this is a rule to add, not one they
  already keep.)
- **Parsing.** Split at the separator; there must be exactly one. Trim white space from each half.
  Strip one leading `+` from the name (the administrator marker; the client strips it too). Lower-
  case the world half.
- **The name half** is 1 to 32 characters, begins and ends with a letter, and holds only letters,
  single spaces, apostrophes and hyphens. A letter is any alphabetic character Windows-1252 holds
  exactly (`Ëlwen` is a name), since other worlds' rules may allow those and every client can show
  them. So a name never holds the separator, a comma, a markup bracket, a control character, or
  `^` or `&` (which the client reads, anywhere in a speaker's name, as the Olthoi speech marks).
- **The world half** is a network slug: 2 to 12 of `a-z` and `0-9`, with single hyphens between
  them. The registry allows slugs of up to 40 characters, but only a world whose slug is a network
  slug may turn federation on ([05-hub.md](05-hub.md)), so that a whole address fits the client:
  `Sir Bartholomew Whitmore@coldeve` (32 characters) is 223 pixels of the friends panel's 258.
  The panel's own add-friend and squelch boxes take 34 characters; longer addresses still work
  through the chat commands, whose line takes 255.
- **Reserved slugs.** No world may take the nine retail worlds' names (`darktide`, `frostfell`,
  `harvestgain`, `leafcull`, `morningthaw`, `solclaim`, `thistledown`, `verdantine`, `wintersebb`),
  nor `channel`, `discord`, `hub`, `net`, `network`, `local`, `localhost`, `world`, `all` or `any`
  (`discord` is kept for Discord itself, [05](05-hub.md) section 5.7, and no world may register
  it). An address may still name one as its world half
  (`Bob@darktide` parses), but since no world can join under it, such an address names no one: a
  tell to it is answered as one to any world not on the network.
- **Identity.** Two addresses are the same identity when they match without regard to case.
  `Address::key` is the address in lower case. The name keeps the case it was written with, for
  display; the world half is always written in lower case.
- **The world always shows.** A remote character is always displayed with its world, so
  `Asheron@vanity` cannot pass as a local member of staff.

### Channels

A channel is addressed at the reserved world `channel`: `oldguard@channel`. A channel name is 2 to
20 of letters, digits and inner single hyphens, matched and written in lower case.

## 2. A character's settings

Every character starts **off the network**. Its world keeps one setting per character, so an alt
or a mule stays off the network unless its owner puts it on too:

| Setting | Values | Says |
|---|---|---|
| on | off, on | whether the character is on the network |

**On the network means visible, reachable and listed**, with no exceptions:

- **Visible.** Anyone on the network who lists the character as a friend sees it online.
- **Reachable.** Anyone on the network may send it a tell.
- **Listed.** Its profile is in the network's directory, published as it changes (below).

`@dn on` puts the character on the network, and `@dn off` takes it off at once: nobody on another
world sees it online or reaches it, and its profile leaves the directory. A linked character's link
to its person pauses while it is off, and resumes at `@dn on` (section 5).

What still keeps a character apart while it is on the network:

- **Squelches.** A character's squelch of a remote character, or of a whole world, holds as ever
  ([03](03-world-behaviour.md) section 5).
- **Blocks.** A world's operator may block a remote character or a whole world for everyone on
  the world.
- **Appearing offline** (the retail character option) makes a character show offline to everyone.
- **Paces.** A world limits how fast any one world reaches its characters
  ([03](03-world-behaviour.md) section 8).

A world that reads a record kept by an earlier version, with more settings than `on`, reads its
`on` alone and writes it back as `on` alone.

### What the directory publishes

While the character is on the network, the network's directory, on the site, shows its profile,
and the hub's public routes answer it ([05](05-hub.md) section 6):

- its address, and its world's name;
- its level, heritage, gender and the title it shows;
- its experience (in all, and not yet spent), its skill credits not yet spent, and its luminance
  (to spend, and the most it may hold);
- its six attributes (as the character was made, and as raised since), its three vitals'
  maximums, its skills (each one's number, training and level) and its augmentations (each one,
  and how many times taken), all without the effects of spells;
- its allegiance: the allegiance's name when its monarch gave it one, how many characters it
  holds, the character's rank in it, and its patron and its monarch while each of them is on the
  network too, so that their pages link to each other;
- only as the character's own options allow, exactly as appraising it in game shows them to
  another player: its date of birth ("Allow others to see your date of birth"), its age ("... your
  age"), its number of deaths ("... your number of deaths"), its chess rank ("... your chess
  rank"), its fishing skill ("... your fishing skill") and its number of titles ("... your number
  of titles");
- the hour its profile last changed.

Never: its inventory, its location, its house, its fellowship, its friends, whether it is online
or when it was last seen, anything of its game account (the account's name, the order of its
characters, an object's id), or its person (section 5): no character's page names, links or
groups another character of the same player.

- **Live.** The character's world sends its profile when it changes, at its login and logout, a
  level gained and any other change to a field above, and the hub publishes it at once
  ([03](03-world-behaviour.md) section 10). **So anyone watching the directory sees when a
  character's profile changes, and so roughly when it plays.** Comparing those times, a determined
  observer may guess that two characters of one world are one player's, since one player plays one
  at a time; and a date of birth shown on two characters made minutes apart says as much. The
  directory never shows whether a character is online.
- **Out at once.** `@dn off` takes the profile out of the directory at once, and the hub deletes
  it.
- **The hub's operator** may take any character out for 30 days. Its world tells the player, and
  sends the profile again only at the character's next login or its next `@dn on`, which the hub
  refuses while the 30 days last ([05](05-hub.md) section 6.4).

## 3. A world's federation flag

The registry keeps one flag per world:

| Flag | Means |
|---|---|
| `on` | The operator turned the network on: the world may hold a link and be addressed. |
| `off` | The operator said no: the hub refuses everything that names the world, from any source. |
| unset | The operator has said nothing: the world's server features stay off (it may not link). |

Server-side features always need the operator's `on`. A world that is not `on` is "not on the
network" to every sender.

## 4. Stand-in ids

A tell, a room line and a friends row each carry a sender id beside the name. A remote character
has no object on the receiving world, so the world gives each remote identity (a character's
address, or a channel's) a **stand-in id**:

- **The block.** Stand-in ids are `0x60000001` to `0x6FFFFFFF`. The final retail client makes a
  tell sender's name a clickable link, and makes the sender its reply target, only for ids from
  `0x50000001` to `0x6FFFFFFF` (observed at runtime); local players use `0x50000001` to
  `0x5FFFFFFF` and nothing uses the block above them.
- **One per identity**, the same for every local player, so a reply by id, a stored friend and a
  squelch all name the same correspondent.
- **Persisted**, so the id survives a relog, a restart and a lost hub link.
- **Never reused**, even after the correspondent is forgotten, so an old id in a client's chat or
  squelch list can never come to mean someone else, **a crash included**: a world saves its next id
  ahead of what it has given (Empyrean saves it 1,024 ahead), so ids given after the last save and
  lost in a crash are never given again.
- Ids are given in order from `0x60000001`. A world that has given them all gives no more: a
  correspondent that would need a new one is not shown (a tell from it is answered `offline`),
  never shown under an id that already stands for someone else.
- **Not for the asking.** A world gives an id only for a line it shows, and limits how many new
  ones another world can cause ([03](03-world-behaviour.md), section 8).

## 5. Persons

A **person** is one Discord account, as the hub knows it. A player who signs in on the site with
Discord may link characters to their person, any number of them on any worlds, one at a time: the
character's world asks the hub for a one-time link on the character's behalf, vouching for it as
it does for a tell, and the player opens the link ([03](03-world-behaviour.md) section 9,
[05](05-hub.md) section 5). The site signs the player in with Discord if they are not, and links
the character with nothing to type. Each linked character belongs to one person. A character
nobody links has no person, and the network treats it exactly as it always did.

**Nothing is hidden.** Anyone who lists a person friend learns which of the network's characters
are that person's. A player who wants a character kept apart keeps it off the network, or does not
link it.

**Off the network, a link pauses.** `@dn off` takes a linked character off the network without
breaking its link: while it is off, its person's friends do not see it, tells to it do not reach the
person's Discord, and the person does not speak as it from Discord. `@dn on` brings all of that back
with nothing to redo. Only `@dn unlink` (or unlinking on the site, or forgetting the person) breaks
the link. The hub takes a character to be on the network while the directory lists it, or its
operator holds it out of the directory ([05](05-hub.md) section 5.4).

**Forgetting the person.** "Forget me" on the site takes every character the person linked off the
network, as `@dn off` on each would, then unlinks them all, ends the person's sessions and deletes
what the hub keeps of the person ([05](05-hub.md) section 5.3). A world that is away when it
happens takes its characters off when it next links.

**Tells into Discord.** A tell to a linked character that is not online reaches the person's
Discord when the character's world says the character would have been shown it, and the person's
reply comes back from that character ([05](05-hub.md) section 5.8). There is no setting to turn it
on: squelches and blocks decide who reaches the character, as in game.

**Friends.** There is one kind of friend, and `@dn friends add Bob@coldeve` (or `@friends add`)
adds it. A character nobody links keeps its friends on its own world, each the one character
added. A linked character's friend is a **person friend**: the person behind the character added,
listed by the hub for the adding character's person, with the character added as its **handle**.

- **It follows the person.** A person friend lists its handle and every other character of that
  person on the network, so the friend is seen on whichever character it plays.
- **It belongs to the person that added it,** and holds for every one of that person's characters.
- **Up to 1,000 across the network.** A person may list up to 1,000 person friends, from all of its
  characters together; adding one more is refused with a plain line. Each character's friends panel
  still shows at most 100 of them, the 6096 client's own limit for its list (section 2 of
  [03](03-world-behaviour.md)): those online first, then those seen most lately. A character nobody
  links still keeps at most 100 remote friends on its own world.
- **Linking carries friends over.** When a character is linked, the friends its world kept for it
  become person friends added from it, each once; any past the person friends that may hold for it
  stay on its world.
- **Unlinking hands them back.** A character unlinked (by `@dn unlink`, on the site, or by
  forgetting the person) keeps the friends that held for it: the hub hands them back to its world,
  each its handle alone, and they are kept there from then on. The person keeps its person friends
  for its other characters. A character moved to another person takes them to that person.
- **A character's own world still answers for it** (section 2): a friend sees it online only while
  it is on the network, not blocked and not appearing offline.

**Who learns what:**

| Who | Learns |
|---|---|
| The player, signed in on the site | their characters and their person friends |
| One of the person's characters, in game | whether it is linked, and its person friends with their characters |
| A character's own world, and its operator | whether its own characters are linked, and the person friends that hold for them, with their characters |
| Any other player or world | through a person friend, the characters of the person it names |
| World operators, through the operator API | nothing |
| The hub's public routes and the site's public pages, the directory included | nothing |

The hub names a person itself (its Discord id or name) to nobody but that person, signed in; the
hub's own operator, who keeps the registry, can read it there.
