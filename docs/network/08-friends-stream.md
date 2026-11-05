# The friends stream

Provenance: **network specification**; the client behavior it serves is **observed at runtime** in
the final Asheron's Call 2 client (30 September 2013).

One implementation: `core/auberean-friends`: `src/frame.rs` (the preamble and the frames),
`xml.rs` (the elements' XML), `element.rs` (every element), `ticket.rs` (the ticket), and the two
halves of a stream, `client.rs` and `server.rs`, which do no I/O. The vectors in
`core/auberean-friends/vectors/friends.json` give every element's text with the values it reads
as, the original friends service's elements in the forms its traffic showed, a stream's bytes,
the tickets, and the text a reader refuses.

The friends stream is the protocol between an Asheron's Call 2 client and its world's friends
service. It carries the service's commands, answers and events as one XML element each, in the
vocabulary the client's friends service always used. Only the transport, the framing, the
handshake and the ticket are this network's own. A client that speaks it, and a world that serves
it, give the player friends with presence, an ignore list, an away state and tells to
`Name@world`, across worlds and games, through the world's link
([02-link-protocol.md](02-link-protocol.md)).

## 1. Why a stream of its own, and why TCP

- **It cannot ride the game connection.** The retail client's friends service is a separate
  component in the client's process, with no access to the game connection. It opens its own
  connection, and it does so before the game login, whose start waits on it.
- **One protocol for every client.** Our own client could carry the same commands over the game
  connection, but that means a second implementation, and a message the game protocol never had.
  Our own client therefore speaks the friends stream too, and the world has one service with one
  set of tests.
- **TCP:**
  - the commands need reliable, ordered delivery, which the original service built over UDP
    itself, and TCP gives free;
  - answers run to kilobytes (a friends list), with no fragmenting of our own;
  - a client behind NAT dials out with nothing to configure;
  - a world already opens its login port to players, and TCP on the same number is one more
    firewall rule;
  - a browser client gets the same elements over a WebSocket (section 9), which is TCP too.
- **Not the original UDP framing.** That framing needs a DirectPlay implementation on both sides,
  which is an optional Windows component and incomplete under Wine.

## 2. Transport and framing

- **Where.** TCP, at the world's address, on the **same port number as its login service** (UDP
  and TCP ports are separate), so that a world's registered endpoint names its friends service
  too. A world that serves the friends stream accepts it there; one that does not leaves the port
  closed.
- **The preamble.** The client's first four bytes are the ASCII `DNF1`. A stream that begins with
  `GET ` is a WebSocket upgrade, which this port does not serve (a browser's stream rides the game's
  WebSocket: section 9): it is closed. Anything else is closed at once, at its first byte that
  differs from both.
- **Frames.** Each frame is a 4-byte little-endian length, then that many bytes of UTF-8: one XML
  element. The length is 1 to 65,536. A longer one, a length of zero, bytes that are not UTF-8, or
  text that is not one well-formed element closes the stream (`Bye Reason="protocol"` first, when
  the reader can still write).

```
44 4E 46 31            DNF1
42 00 00 00            66 bytes follow
3C 48 65 6C 6C 6F 20   <Hello Proto="1" Software="auberean-friends/0.1.0" Locale="1033"/>
```

- **Elements.**
  - An XML 1.0 element with quoted attributes. Text is escaped with `&amp;` `&lt;` `&gt;`
    `&quot;` `&apos;` and numeric references.
  - No declaration, document type, comment, processing instruction or CDATA, and no text inside
    an element.
  - Children one level deep at most, each an empty element with attributes.
  - A reader ignores an attribute it does not know, and skips an element it does not know without
    an error, so that a later version can add them. It ignores a child it does not know, too.
  - Names and text are Unicode. The client turns them into whatever its own strings hold.
- **What a reader takes.** White space around the element, before `>` and `/>`, around `=` and
  between children, as XML allows: the original service wrote `<FriendStat User="…" Mode="0" />`
  and a list as `<FStart Cnt="2" >…</FStart> `. Single quotes as well as double. A tab, line feed
  or carriage return written as itself in an attribute reads as a space, as XML has it; written as
  a reference it is itself.
- **What a writer writes.** Double quotes; one space before each attribute and no other white
  space; an element without children as `<Name …/>`. `&`, `<`, `>` and `"` as named references, and
  a tab, line feed and carriage return as `&#9;`, `&#10;` and `&#13;`. Characters XML cannot carry
  at all (the other control characters) are left out.
- **Values.** Numbers are decimal digits, with no sign. `Cx` is `0x` and 1 to 16 hex digits,
  `HR` is `0x` and 1 to 8; both are written in lower case, as the original service wrote them, and
  read in either case. A known element missing an attribute it needs, or holding a value out of
  its bounds, is malformed, and closes the stream as text that is no element does.

## 3. The handshake and the ticket

| step | from | element | means |
|---|---|---|---|
| 1 | client | `<Hello Proto="1" Software="auberean-friends/0.1.0" Locale="1033"/>` | the first frame. `Proto` is the version spoken (1). `Software` is 1 to 64 printable ASCII characters. `Locale`, optional, is the client's Windows locale number |
| 2 | world | `<Welcome Proto="1" World="dawnsong" Name="Dawnsong" Ping="20"/>` | `World` is the world's network slug (empty for a world not on a network); `Name` is the world's name as its login shows it; `Ping` is the keep-alive interval in seconds, at least 1. A `Proto` the world does not speak, or any first element but `Hello`, is answered `Bye Reason="protocol"` |
| 3 | client | `<PresentTicketCmd Cx="0x154218" Email=""><Ticket Val="…"/></PresentTicketCmd>` | the ticket, as given to the client, 1 to 512 printable ASCII characters. `Email` is ignored |
| 4 | world | `<PresentTicketRes Cx="0x154218" HR="0x0" Status="0"/>` | admitted, or a failure code (section 8). `Status` is `0` |

- **The ticket** is the game login's own account and secret, wrapped:

  ```
  a1.<account, UTF-8, URL-safe base64 without padding>.<secret, UTF-8, URL-safe base64 without padding>
  ```

  The account is not empty. Base64 here is RFC 4648's URL-safe alphabet, without `=`, with the
  bits past the last whole byte zero, so that each ticket has one spelling. The form holds only
  `A–Z a–z 0–9 - _ .`: safe on any command line.
- **Clients pass the ticket through untouched.** Only the world reads it. A ticket of another
  form is refused like a wrong secret, so that a later form changes the launcher and the world,
  never the clients.
- **The ticket admits exactly whom the world's game login admits.** The world checks the account
  and its secret with the same rule as its login (an open world makes the account the first time
  its name comes, with that secret, and admits only that secret after; a world of listed accounts
  admits only an account made beforehand, with its own). It refuses an account its login would refuse, and a
  banned one with the banned code. The ticket proves what the login proves, no more and no less.
- **The ticket is a secret.** It is the password, and nothing logs it.
- **The stream is bound to the account.** It acts for the one character the account has in the
  world. Until the account has one, every request is answered with the not-connected code
  (section 8), and no event is sent. When a character enters, the world sends `FStart` with that
  character's friends (section 6). When it leaves, nothing is sent; the stream stays, and the
  next character to enter gets its own `FStart`. An answer to a request made while the character
  was in the world is still sent after it leaves.
- **A second stream for the same account replaces the first.** The first is sent
  `Bye Reason="replaced"` and closed.
- **Requests before the ticket** are answered with the not-ticketed code. A second ticket on a
  stream, while the first is being checked or after it was admitted, gets the already-ticketed
  code. After a refused ticket the client may present another, within the ticket's deadline.

## 4. Keep-alive, limits, closing

- **Keep-alive.** Each side sends `<Ping Seq="n"/>` after `Ping` seconds (20) of not sending, and
  answers each with `<Pong Seq="n"/>` carrying the same number. Each side closes a stream it has
  heard nothing on for 70 seconds, with `Bye Reason="idle"`. The elements are the protocol's own,
  not the transport's, so that a browser, which cannot send WebSocket pings, keeps the same rule.
- **Deadlines.** A world closes a stream without `Hello` within 10 seconds of opening, and one
  without a ticket within 30 seconds of `Welcome`, with `Bye Reason="idle"`. A connection that has
  not sent the preamble within 10 seconds is closed without a word. A client gives up a stream the
  world has not welcomed within 10 seconds of opening, with `Bye Reason="idle"`, and reconnects as
  after any other end (section 5): the original client waits for its friends service's answer with
  no limit of its own.
- **Bounds.**
  - At most 8 streams from one address (an IPv6 one counted by its /64), and a ceiling for the
    world (Yalain: 2,000).
  - Elements: a burst of 20, then 5 a second, counting every element a stream sends after its
    preamble, its greeting included. A stream past that is sent `Bye Reason="flood"` and closed;
    a client's own use stays far below it.
  - Tells are also held to the network's per-character pace
    ([02-link-protocol.md](02-link-protocol.md) section 5).
  - Five refused tickets from one address in a minute: the next minute's are refused without
    being checked, so the stream is no easier to guess passwords on than the login.
- **Closing.** `<Bye Reason="…"/>` names why, then the sender closes; the other side does not
  answer it. The reasons are `protocol`, `replaced`, `refused`, `flood`, `idle`, `full` and
  `shutdown`. A reader takes an unknown or missing reason as `protocol`. A client that stops says
  `shutdown`.

## 5. Reconnecting

- **The client** reconnects after a stream ends, or after an attempt to connect fails, waiting
  1, 2, 5, 10, then every 30 seconds between attempts. A stream that stood 60 seconds from when
  its connection opened starts the waits again from 1. After `replaced` it waits the longest step,
  30 seconds, each time, so that two clients of one account do not take turns.
- **On the new stream** it says `Hello` and, once welcomed, presents the same ticket. The ticket
  stays good for the whole session, since it is the game login's own.
- **The world** then sends `FStart` again, after its answer to the ticket, if the account's
  character is in the world. The world queues nothing for a client that is away: a tell to a
  character whose stream is down is answered as to one not reachable.
- **Every request gets one answer.** A client answers a request of its own player's that it
  cannot send, because the stream is not up, and every request still waiting when a stream ends,
  with the not-connected code, so that its friends layer is never left waiting. A request holding
  a value out of its bounds is never sent: the client answers it with `0x80070057`. An answer that
  matches no request the client is waiting on (a second answer for the same `Cx`, one of another
  kind, or one for a `Cx` it never sent) is dropped, so that each request is answered exactly once.

## 6. The elements

Every request carries `Cx`, the client's mark for the call: `0x` and 1 to 16 hex digits. The
answer echoes it and adds `HR`, a result code (section 8). Events carry no `Cx`. Names are
addresses (section 7). Element and attribute names are those the original service used where its
traffic shows them; the rest are this protocol's.

| request (client to world) | attributes and children | answer | notes |
|---|---|---|---|
| `PresentTicketCmd` | `Cx Email` + `<Ticket Val/>` | `PresentTicketRes Cx HR Status` | section 3 |
| `GetFriendsList` | `Cx` | `GetFriendsListRes Cx HR Cnt` + `Cnt` × `<FriendStatus User Mode Location/>` | `Location` empty |
| `FriendAdd` | `Cx User` | `FriendStat User Mode` (the event), then `FriendAddRes Cx HR` | the event before the answer, as the original did |
| `FriendDelete` | `Cx User` | `FriendDeleteRes Cx HR` | |
| `GetIgnoredList` | `Cx` | `GetIgnoredListRes Cx HR Cnt` + `Cnt` × `<Jerk Str/>` | `Str` is the address |
| `IgnoredAdd`, `IgnoredDelete` | `Cx User` | `IgnoredAddRes`, `IgnoredDeleteRes` `Cx HR` | `User` is this protocol's attribute |
| `Flags` | `Cx Op Word Flags` | `FlagsRes Cx HR WhichFlag Flags` | the client's start-of-session state flags; the world echoes `Word` as `WhichFlag`, and `Flags`, and keeps nothing |
| `SetLoc` | `Cx Flags Location` | `SetLocRes Cx HR` | `Flags` 2 away, 1 available, 4 not to be disturbed; `Location` ignored |
| `SendToUserEx` | `Cx User Text` + optional `<Extra Val/>` | `SendToUserExRes Cx HR` | a tell; `Text` 1 to 1,024 characters; `Extra` is 32 hex digits the client attaches, carried to a recipient on the same world and dropped across worlds |
| `CallCreate` | (reserved) | `CallCreateRes Cx HR` | not served in this version: the not-implemented code |
| `SendToRoomEx` | (reserved) | `SendToRoomExRes Cx HR` | rooms are the game's chat rooms, never this service's: the not-implemented code |

| event (world to client) | attributes | when |
|---|---|---|
| `FStart` | `Cnt` + `Cnt` × `<FriendStatus User Mode Location/>` | the character enters the world (or the stream comes back while it is in): every friend with its state |
| `FriendStat` | `User Mode` | one friend's state changed, or a friend was just added |
| `UTellEv` | `User Text` + optional `<Extra Val/>` | a tell to the character; `User` is the teller |

- **`Mode`** is a bit set: 0 offline; 1 online; 2 away; 4 not to be disturbed. An online friend
  away is `3`, and one not to be disturbed is `5`. A friend on another world is only ever 0 or 1,
  since the link's presence is online or not.
- **`Cnt`** is written as the number of children. A reader goes by the children.
- **Optional attributes.** `Email` and `Location` read as empty when left out; `Locale` as none.
- **What the world's stream answers itself**, with no world behind it: a request before the ticket
  (not ticketed), a second ticket (already ticketed), any request while the account has no
  character in the world (not connected), `Flags` (echoed), and `CallCreate` and `SendToRoomEx`
  (not implemented).

## 7. Names on the stream

- **From the client**, `User` is a name as the client wrote it. The world reads it as an address.
  A name whose world part is the world's own name (in any case, spaces kept) or its slug is a
  local character. A name with no world part is local too.
- **From the world**, a local character is written `Name@` and the world's name exactly as its
  login named it, so that the client recognizes its own world. Every other character is written as
  its network address, `Name@slug` ([01-addresses-and-settings.md](01-addresses-and-settings.md)).
  A name is written as its world writes it (case kept), and two names are one identity when they
  match without regard to case.
- **A name that is no address** (two separators, a bad name half, an unknown own-world character)
  is answered with the no-such-user code.

## 8. Result codes

`HR` is written `0x` and hex digits, `0x0` for success. These are the original service's codes, so
that the client shows its own messages for them.

| code | means |
|---|---|
| `0x0` | done |
| `0xc1120806` | the ticket is refused: the client ends its login, as for a refused password |
| `0xc1121005` | the account is banned |
| `0xc112080b` | a request before the ticket |
| `0xc112080c` | a second ticket on the stream |
| `0xc1120405` | not connected: the account has no character in the world now; from the client itself, the stream is down |
| `0xc1120012` | no such user: the name is no address, or names no character of this world |
| `0xc1120011` | not online: the user is not logged in now |
| `0xc1120010` | timed out: no answer in time, or the user's world cannot be reached |
| `0xc1120018` | denied: a tell held back, a friend who will not be added |
| `0x80004001` | not implemented in this version (`CallCreate`, `SendToRoomEx`) |
| `0x80070057` | from the client itself: a request it would not send, a value out of its bounds |

**A tell's answer** is what the teller sees. With `0x0` the client shows only its own echo of the
tell. With `0xc1120011` it says the name, as the player typed it, is not logged in right now; with
`0xc1120012`, that the name does not exist. Every other code gives one line, that the message could
not be sent right now: `0xc1120010` and `0xc1120018` read sensibly in a log. Which code answers each
outcome of a tell (delivered, away, ignored, not reachable, held back) is part of how an Asheron's
Call 2 world behaves, not of the stream ([07](07-ac2-world-behaviour.md) section 4).

## 9. The WebSocket binding

A browser can open no TCP connection, and a world's game WebSocket is already there, with its TLS,
its allowed origins and its limits. So the stream rides that connection by a port of its own:

- **The port.** `P + 4` in the connection's numbering of its ports
  ([05-websocket-frame.md](../networking/05-websocket-frame.md)). The game's ports are `P` to `P + 3`.
- **Frames.** The stream's bytes in order, with its preamble and its frames as on TCP, in binary
  messages of any size; messages are no boundary of the stream's. An empty message ends the stream,
  from either side.
- **Its first bytes open it,** and it ends when the connection does. The same bounds, handshake and
  keep-alive.
- **Where it may go.** `wss://` to any host, `ws://` only to this machine, as a world's game
  WebSocket ([02-link-protocol.md](02-link-protocol.md) section 2): the ticket rides it. Elsewhere a
  browser client goes without friends.
- **A server without it** drops the port, as a relay to a world's UDP ports does. A browser client
  takes a stream with no answer in 3 seconds as not served, so its login waits no longer.
- **Who uses it.** A browser client, which has nothing else.

## 10. Who speaks it

- **Auberean**, the Asheron's Call 2 client, speaks it, over TCP on the desktop and over the
  WebSocket in a browser. It does not carry the commands over its game connection.
- **The client library `auberean_friends.dll`** speaks it for the unmodified retail client, which
  loads it in place of its original friends component.
- **The world** has one friends service behind every transport.
