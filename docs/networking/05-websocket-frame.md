# The WebSocket frame

**Provenance: this project's own extension.** Retail speaks UDP only. Nothing here is retail's or
ACE's: it is how a browser client, which cannot send UDP, carries the same datagrams, and how an
Empyrean server with WebSocket enabled takes them. The datagrams themselves (their header,
checksums, ISAAC and everything above) are exactly those of `01-packet-format.md` onward.

The code is `dereth_transport::web_frame`.

## Summary

One binary WebSocket message carries one datagram, behind a two-byte big-endian port. A
connection names the server's two ports by the first datagram it sends, and port 0 carries the
session's **will**: the datagrams to deliver for it if its WebSocket closes.

Two things speak it on the server side:

- **An Empyrean server** with `[server.websocket]` enabled accepts the connection itself. Each
  WebSocket is one client endpoint of the server, beside its UDP clients.
- **`dereth-web-relay`**, a proxy the player runs on their own machine, carries the datagrams to a
  server that speaks only UDP (ACE, GDLE, or an Empyrean without WebSocket).

The client does the same thing in both cases.

## 1. A message

| Bytes | Meaning |
|---|---|
| 0–1 | The port, big-endian `u16`. |
| 2… | One datagram, as it would be sent over UDP: header, optional sections, fragments. |

- **Only binary messages carry frames.** Text messages are ignored.
- **A message shorter than two bytes is ignored.**
- **One message per datagram,** in both directions. Nothing is batched and nothing is split.

## 2. The two ports

A session uses two server ports: the logon port **P**, where its datagrams go, and **P + 1**,
which the connect response comes from and the client's reply to it goes to (see
`03-connection-state-machine.md`). The frame keeps the two apart by number:

- **The first datagram a connection sends** is the login, so its port is the logon port *as the
  client numbers it*. From then on that port and the one above it are the connection's two ports.
  Any other port is dropped.
- **A datagram coming back** carries the port it came from, in the same numbering: P for the logon
  port and P + 1 for the one above.
- **The server or relay maps the pair onto its real ports.** A client may number them as it likes;
  the browser client uses 9000 and 9001 whatever the server listens on. So a page needs a URL and
  nothing else.

## 3. The will (port 0)

A message addressed to port 0 is not a datagram. Its body lists the datagrams to deliver for the
session when its WebSocket closes, however it closes: a close handshake, a dropped connection or an
idle timeout. Each entry is:

| Bytes | Meaning |
|---|---|
| 0–1 | The datagram's length *n*, big-endian `u16`. |
| 2–3 | Its port, big-endian `u16`, in the connection's numbering (§2). |
| 4…4+*n* | The datagram. |

The rules:

- **A later will replaces an earlier one,** and a will with no entries clears it.
- **Malformed entries:** a trailing entry that claims more bytes than follow is dropped. An entry
  to a port outside the pair is dropped.
- **Delivery on close:** the will's datagrams are delivered exactly as if the client had sent them,
  in order, and only once.

The client keeps its will current with the datagrams that log it off and disconnect, so a page
that is closed or crashes still leaves the world at once, rather than after the session timeout.

## 4. Where it is served

**An Empyrean server** serves the frame on the path of its choosing (any request path is accepted)
at `wss://host:port/`, or at `ws://` where plain WebSocket is allowed. `SETUP.md` describes the
configuration, a reverse proxy in front of it, and the limits it applies. The server's status
endpoint reports the URL as `websocket_url`, so a launcher or a page can find it.

**`dereth-web-relay`** serves it at `ws://127.0.0.1:<port>/`, and only there.

**Refusals.** Both refuse a browser page whose `Origin` they were not told to accept, and a
connection without an `Origin` header is not a browser page. The browser client refuses a `ws://`
URL to anything but the loopback address, because the game protocol is not encrypted: ISAAC only
keys the checksums, and the login carries the account's password.
