# dereth-web-relay

The local proxy a player runs to play in a browser on a server that speaks only UDP (ACE, GDLE,
or an Empyrean with its WebSocket endpoint off). A browser cannot send UDP; the web client sends
each game datagram as one WebSocket message, and this relay, on the player's own machine, carries
it to the server over UDP and the answers back.

An Empyrean server with WebSocket enabled needs no relay: the web client connects to it directly.

## Use

```text
dereth-web-relay --server <host:port> [--listen 127.0.0.1:9180] [--allow-origin <origin>]
```

- `--server`: the game server, by address or name, and its logon port (the one a desktop client is
  given, 9000 by default). A name is looked up once, at start.
- `--listen`: where the web client connects; loopback only. The default is `127.0.0.1:9180`.
- `--allow-origin`: the web client's page, when it is not served from this machine, for example
  `https://play.example.org`. One origin: a scheme, a host and an optional port.

Then give the web client the server URL `ws://127.0.0.1:9180/`.

Build it with `cargo build --release -p dereth-web-relay`; the binary is
`target/release/dereth-web-relay`. It runs on Windows, macOS and Linux.

## What it will and will not do

It opens a hole through the browser's sandbox, so it is kept the size of one game session:

- **Loopback only.** It listens on `127.0.0.1` (or `::1`) and refuses any other address.
- **One server.** It sends only to the server it was started for.
- **Two ports.** It sends only to that server's logon port and the one above it, whatever ports
  the page asks for.
- **Local pages, and the one you name.** A page served from this machine (`localhost`,
  `127.0.0.1`, `[::1]`) may connect, and the page named with `--allow-origin`; every other page is
  refused.
- **The will.** Each session hands over, ahead of time, the datagrams that log it off. When its
  WebSocket closes, however it closes, the relay sends them, so closing the tab leaves the world at
  once rather than after the server's timeout.

Each browser session gets its own UDP socket, so the server sees one client per session. The frame
is specified in `docs/networking/05-websocket-frame.md`.

## A page served from elsewhere

A web client served over `https://` from another site can reach `ws://127.0.0.1:9180/`:

- **The origin.** Start the relay with `--allow-origin <that page's origin>`, or it refuses the
  connection.
- **Mixed content.** The browsers treat the loopback address as a secure origin, so a plain `ws://`
  connection to `127.0.0.1` or `localhost` from an `https://` page is not blocked as mixed content.
- **Local network access.** Chromium browsers are moving to ask the player's permission before a
  public site may reach the local network or loopback. Allow it for the page's site when asked.

So far the relay has been run with a page served from the same machine: Safari on macOS, and
Microsoft Edge on Windows (login, and the will sent when the tab closed). A page served from
another site has not yet been tried in each browser.
