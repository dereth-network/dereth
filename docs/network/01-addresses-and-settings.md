# Addresses, settings and stand-in ids

Provenance: **network specification**, shaped by what the final retail client (June 2015) does,
**observed at runtime** where noted.

One implementation: `network/protocol/src/address.rs`, `settings.rs` and `standin.rs`. The vectors
in `network/conformance/vectors/addresses.json` pin the rules below.

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
  (`discord` is kept for speakers from Discord, [05](05-hub.md) section 5.6). An address
  may still name one as its world half (`Bob@darktide` parses), but since no world can join under
  it, such an address names no one: a tell to it is answered as one to any world not on the
  network.
- **Identity.** Two addresses are the same identity when they match without regard to case.
  `Address::key` is the address in lower case. The name keeps the case it was written with, for
  display; the world half is always written in lower case.
- **The world always shows.** A remote character is always displayed with its world, so
  `Asheron@vanity` cannot pass as a local member of staff.

### Channels

A channel is addressed at the reserved world `channel`: `oldguard@channel`. A channel name is 2 to
20 of letters, digits and inner single hyphens, matched and written in lower case.

## 2. A character's settings

Every character starts **off the network**. Its world keeps, per character (so an alt or a mule
stays hidden unless its owner opts it in too):

| Setting | Values | Says |
|---|---|---|
| on | off, on | whether the character is on the network at all |
| Visible | `nobody`, `friends`, `channels`, `anyone` | who sees it online |
| Reachable | `nobody`, `friends`, `anyone` | who may send it a first tell |
| Listed | `off`, `on` | reserved for a later version, which will say whether its profile (name, level, heritage, title; never its inventory) is published; the first version stores it and publishes nothing |

- A character that never opted in has **every setting off**: off the network, Visible `nobody`,
  Reachable `nobody`, Listed `off`.
- **`/dn on` the first time** gives on, Visible `friends`, Reachable `anyone`, Listed `off`: mutual
  friends see it online, and anyone on the network may send it a tell. Opting in again later keeps
  the settings it had.
- **`friends` means mutual friends**: the character lists the other, and the other lists it. A
  world can check only its own half; the other half is the other world's word (a tell's `friend`
  flag, or its sending a watch), which it cannot verify. A character lists its friends: those its
  world keeps for it and, while it is online, the characters its person friends list (section 5).
- **`channels`** means mutual friends and the members of the character's channels. A world that does
  not know its characters' channels' members treats it as `friends`.
- **The reply window.** A character may always be answered by anyone it sent a tell to within the
  last 30 minutes, whatever its Reachable setting, so a conversation it starts is never one-sided.
- **Appearing offline** (the retail character option) makes a character show offline to everyone.

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

## 5. Persons and alts

A **person** is one Discord account, as the hub knows it. A player who signs in on the site with
Discord may link characters to their person, any number of them on any worlds, one at a time: the
character's world asks the hub for a one-time code on the character's behalf, vouching for it as it
does for a tell, and the player enters the code on the site ([03](03-world-behaviour.md) section 9,
[05](05-hub.md) section 5). Each linked character belongs to one person. A character nobody links
has no person, and the network treats it exactly as it always did.

The person has one setting of its own, kept by the hub:

| Setting | Values | Says |
|---|---|---|
| alts | `hidden` (the default), `shown` | whether anyone else may learn that the person's characters are one person |

- **Hidden.** Nobody learns that two characters are one person: not other players, not other
  worlds or their operators, not a character's own world, not the site's public pages. Every
  answer about a character of a hidden person is the answer about a character nobody has linked.
- **Shown.** Anyone may learn which of the network's characters are the person's: the hub names
  them to a world that asks about any one of them. The person's friends then hold for every one of
  its characters. Only the person shows them, signed in on the site; a world, which speaks for its
  characters and so could speak falsely for one, may hide them, never show them.

**Friends.** There is one kind of friend, and `/dn friends add Bob@coldeve` (or `@friends add`)
adds it. A character nobody links keeps its friends on its own world, each the one character
added. A linked character's friend is a **person friend**: the person behind the character added,
listed by the hub for the adding character's person, with the character added as its **handle**.

- **It follows the person, if the person allows it.** A person friend lists its handle and, while
  the handle's person shows its alts, every other character of that person, so the friend is seen
  on whichever character it plays. While the handle's person hides its alts, it is exactly a
  friend of that one character.
- **It belongs to the person that added it.** While that person's alts are hidden it holds for the
  character it was added from alone, and while they are shown, for all of its characters.
- **Linking carries friends over.** When a character is linked, the friends its world kept for it
  become person friends added from it, each once; any past the person friends that may hold for it
  stay on its world.
- **Unlinking hands them back.** A character unlinked (by `/dn unlink`, on the site, or by
  forgetting the person) keeps the friends that held for it: the hub hands them back to its world,
  each its handle alone, and they are kept there from then on. The person keeps its person friends
  for its other characters. A character moved to another person takes them to that person.
- **It never makes a character visible.** Each character's own world still answers for it by its
  own settings (section 2), so a character whose Visible is `friends` shows to a watcher only when
  the two are mutual friends, as a person friend on each side makes them.

**Who learns what:**

| Who | With the person's alts hidden | With them shown |
|---|---|---|
| The player, signed in on the site | their characters, the setting, their person friends | the same |
| One of the person's characters, in game | whether it is linked, and the setting; never the person's other characters, which would pass through its world | also the person's characters |
| A character's own world, and its operator | whether its own characters are linked, the setting, and the person friends that hold for that character; never that two characters are one person | also the person's characters, and all its person friends |
| Any other player or world | nothing: every answer is the one for a character nobody has linked | the person's characters |
| World operators, through the operator API | nothing | nothing |
| The hub's public routes and the site's public pages | nothing | nothing |

The hub names a person itself (its Discord id or name) to nobody but that person, signed in; the
hub's own operator, who keeps the registry, can read it there.
