# dereth-web-relay

The proxy that lets a browser play on a server that speaks only UDP (ACE, GDLE, or an Empyrean
with its WebSocket endpoint off). A browser cannot send UDP; the web client sends each game
datagram as one WebSocket message, and the relay carries it to the server over UDP and the answers
back.

It runs one of two ways:

- **Beside the server, for many players.** The world's operator runs it next to their ACE or GDLE
  server, behind TLS. Players need nothing installed: the web client connects to the relay's
  `wss://` address, which the Dereth Network hub lists for the world, or which a player adds by
  hand.
- **On a player's own machine**, for that player alone, on loopback.

An Empyrean server with WebSocket enabled needs no relay: the web client connects to it directly.

Build it with `cargo build --release -p dereth-web-relay`; the binary is
`target/release/dereth-web-relay`. It runs on Windows, macOS and Linux.

## Beside your server (operators)

### The command

On the machine that runs ACE (logon port 9000), behind a TLS proxy on the same machine:

```text
dereth-web-relay --server 127.0.0.1:9000 --public --trusted-proxy 127.0.0.1
```

Then point your proxy's `wss://relay.example.org/` at `http://127.0.0.1:9180` (below), and tell
players the address, or let the hub list it.

| flag | what it does |
|---|---|
| `--server <host:port>` | the game server and its logon port (ACE's 9000, GDLE's 9050 by default). The relay sends to that port and the one above it, nothing else. |
| `--public` | runs it for many players. Without it, it is a player's relay. |
| `--listen <addr:port>` | where browsers (or your proxy) connect. `127.0.0.1:9180` by default; any other address needs `--tls-cert`/`--tls-key` or a `--trusted-proxy`, since a browser reaches another machine only over `wss://`. |
| `--allow-origin <origin>` | another web page allowed to connect, besides the Dereth web client's (`https://play.dereth.network`, always allowed). Repeat it for more. |
| `--tls-cert <pem> --tls-key <pem>` | serves `wss://` itself with this certificate chain and key, in place of a proxy. |
| `--trusted-proxy <addr or cidr>` | a proxy in front, whose header names the player's address. Repeat it for more. |
| `--client-address-header x-forwarded-for\|cf-connecting-ip` | which header that is; `x-forwarded-for` by default, read from the right. |
| `--proxy-hops <n>` | how many trusted proxies a request crosses (2 for Cloudflare and then your own proxy); 1 by default. |
| `--max-per-ip <n>` | sessions at once from one player's address (an IPv6 one by its /64); 8 by default. |
| `--max-sessions <n>` | sessions at once in all; 100 by default. Keep it at or below your server's own limit. |
| `--world <slug> --public-url <wss url>` | announces the relay to the hub (below). |
| `--hub <url>` | the hub to announce to; `https://hub.dereth.network` by default. |
| `--key-file <path>` | the file holding the world's registry key; else `DERETH_NETWORK_KEY`. |

Each line of its log starts with the UTC time. Run it as a service (systemd, or a Windows service
wrapper) so it starts with the machine.

### TLS: a reverse proxy, or the relay's own certificate

A page served over `https://` (the Dereth web client is) can only open `wss://`. Choose one:

**A reverse proxy** holds the certificate and passes WebSocket upgrades on. The relay listens on
loopback and trusts the proxy's `X-Forwarded-For`:

```caddy
relay.example.org {
	reverse_proxy 127.0.0.1:9180
}
```

Caddy obtains the certificate itself and passes WebSockets on with no idle timeout. With nginx:

```nginx
location / {
    proxy_pass http://127.0.0.1:9180;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_read_timeout 120s;
}
```

**The relay's own certificate**, for a machine with no proxy:

```text
dereth-web-relay --server 127.0.0.1:9000 --public --listen 0.0.0.0:9443 \
    --tls-cert /etc/letsencrypt/live/relay.example.org/fullchain.pem \
    --tls-key /etc/letsencrypt/live/relay.example.org/privkey.pem
```

The address is then `wss://relay.example.org:9443/`. The files are read at start: restart the
relay after the certificate is renewed.

### Cloudflare in front

WebSockets work through Cloudflare, proxied DNS and tunnels alike.

- **A tunnel** (`cloudflared` on the same machine, no open port, no certificate of your own): give
  the tunnel a public hostname, `relay.example.org`, with the service **HTTP** at
  **`127.0.0.1:9180`**, and run the relay with
  `--trusted-proxy 127.0.0.1 --client-address-header cf-connecting-ip`. Name the address, not
  `localhost`, which may resolve to `::1`. Only Cloudflare reaches the relay this way, so its
  header is safe to believe.
- **Proxied DNS** (the orange cloud) to your own proxy: trust both, Cloudflare's published ranges
  and your proxy, and read two hops:
  `--trusted-proxy 127.0.0.1 --trusted-proxy 173.245.48.0/20 ... --proxy-hops 2`.

Cloudflare closes a WebSocket that stays silent for long; a game session is never silent for more
than a few seconds.

### Ports and the firewall

- **Open** the port players connect to: 443 on the proxy, or the relay's own `--listen` port. With a
  tunnel, none.
- **The relay's own port** (9180 by default) stays closed to the internet when a proxy is in
  front.
- **The game server's UDP ports** (ACE's 9000 and 9001) need no opening for web players when the
  relay is on the same machine: it reaches them on loopback. Desktop players still need them. A
  relay on another machine needs to reach both over UDP.

### The hub

With the world registered at the hub, the relay can announce its address, and the hub's list of
worlds then names it; the web client shows the world as reached directly and connects with no
step for the player:

```text
DERETH_NETWORK_KEY=drth_... dereth-web-relay --server 127.0.0.1:9000 --public \
    --trusted-proxy 127.0.0.1 --world coldeve --public-url wss://relay.example.org/
```

The key is the world's registry key, the one ACE's network mod reads from the same variable. Put
it in the environment or in a file (`--key-file`), never on the command line, which every user of
the machine can read: the relay refuses a key there. The relay announces every minute; the hub
lists the address for three minutes after each, so a relay that stops is delisted soon after. A
world whose link to the hub names a WebSocket address of its own (an Empyrean with its endpoint
on) is listed with that one.

Without the hub nothing changes: players add the world on the web client's front page by its
`wss://` address.

### What your game server sees

Every web player reaches the server from the relay's address (`127.0.0.1` when it runs on the
same machine), each session from a UDP port of its own.

- **ACE's per-address limit** (`MaximumAllowedSessionsPerIPAddress` in `Config.js`, unlimited by
  default) would count every web player as one address. If you set it, add the relay's address to
  `AllowUnlimitedSessionsFromIPAddresses` and let the relay's `--max-per-ip` keep the same rule by
  the players' real addresses.
- **ACE's `MaximumAllowedSessions`** (128 by default) counts web players with the rest; keep the
  relay's `--max-sessions` at or below it.
- **Addresses in ACE's records** (an account's `CreateIP` and `LastLoginIP`) are the relay's for a
  web player. The relay's log has the player's: each session's line names the player's address,
  the UDP address the server sees it from (ACE's own log names a login by it, `Login Request from
  127.0.0.1:54321`) and the account its login names:

  ```text
  2026-11-01T18:02:11Z 203.0.113.5 (through 127.0.0.1:61022): session on udp 127.0.0.1:54321 (3 open)
  2026-11-01T18:02:11Z 203.0.113.5 (through 127.0.0.1:61022): logging in as "ironbeard" from udp 127.0.0.1:54321
  ```

  So for a misbehaving character: `@finger <character>` names its account, and the relay's log
  names the address that account came from.
- **Bans** in ACE (`@ban`) and GDLE are by account, which works the same for web players. Neither
  server bans by address. To keep an address out of the web, block it in front of the relay (your
  proxy or Cloudflare).
- **Accounts:** with ACE's `AllowAutoAccountCreation` (GDLE's `auto_create_accounts`), anyone who
  reaches the relay can make an account, as anyone who reaches the UDP port can.

## On your own machine (players)

```text
dereth-web-relay --server <host:port> [--listen 127.0.0.1:9180] [--allow-origin <origin>]
```

- `--server`: the game server, by address or name, and its logon port (the one a desktop client is
  given, 9000 by default). A name is looked up once, at start.
- `--listen`: where the web client connects; loopback only. The default is `127.0.0.1:9180`.
- `--allow-origin`: the web client's page, when it is not served from this machine, for example
  `https://play.example.org`. One origin: a scheme, a host and an optional port.

Then give the web client the server URL `ws://127.0.0.1:9180/`.

A web client served over `https://` from another site can reach `ws://127.0.0.1:9180/`:

- **The origin.** Start the relay with `--allow-origin <that page's origin>`, or it refuses the
  connection.
- **Local network access.** Chromium browsers (147 and later) and Firefox (151 and later) ask the
  player's permission before a public site may reach loopback. Allow it for the page's site when
  asked.
- **Safari** treats `ws://127.0.0.1` from an `https://` page as mixed content and blocks it, with
  no permission to give; a relay on the player's own machine cannot serve Safari from an
  `https://` page. A relay beside the server can.

## What it will and will not do

It opens a hole through the browser's sandbox, so it is kept the size of a game session:

- **One server, two ports.** It sends only to the server it was started for, and only to its
  logon port and the one above it, whatever ports the page asks for. It cannot be an open proxy.
- **Named pages only.** A player's relay admits pages served from that machine and the one named
  with `--allow-origin`. An operator's admits only the Dereth web client's origin and those named,
  and refuses a request with no `Origin`: every browser sends one, so one without is a program,
  and a program can speak UDP to the server itself; the relay would only lend it its own address.
  Origins stop other web pages from using a player's browser; they cannot stop a program that
  writes the header, which is what the limits below are for.
- **Listening.** A player's relay listens on loopback only. An operator's listens elsewhere only
  when told to, and then only with TLS of its own or a trusted proxy in front.
- **Addresses.** A forwarded header is read only from a trusted proxy; from anyone else it is
  ignored, so no one can pose as many addresses. One the trusted proxy sends that does not read is
  refused.
- **Limits** (an operator's relay): sessions at once per address and in all (`--max-per-ip`,
  `--max-sessions`); 8 connections in their handshakes at once from one address and 64 in all, 10
  seconds for the TLS and WebSocket handshakes together; 200 datagrams a second (400 at once) and
  128 KiB a second (256 KiB at once) from each session, the rest dropped; a session silent for 60
  seconds is closed; a frame is at most 128 KiB, and a page that stops reading is cut off once 1 MiB
  waits for it.
- **The will.** Each session hands over, ahead of time, the datagrams that log it off. When its
  WebSocket closes, however it closes, idle included, the relay sends them, so closing the tab
  leaves the world at once rather than after the server's timeout.
- **The key** to announce to the hub is read from a file or the environment, sent only in the
  request's `Authorization` header, and never written to the log.

Each browser session gets its own UDP socket, so the server sees one client per session. The frame
is specified in `docs/networking/05-websocket-frame.md`.

## Limits

- **Safari and a player's own relay:** see above. Safari players play through a relay beside the
  server, or a world with a WebSocket endpoint of its own.
- **Scale:** one thread per session, each waking every 2 ms to carry what came in, as Empyrean's
  own WebSocket endpoint does.
- **Status:** a browser cannot ask a UDP server whether it is up, and the relay does not answer for
  it; the hub's heartbeat (ACE's network mod) does.

So far a player's relay has been run with a page served from the same machine: Safari on macOS,
and Microsoft Edge on Windows (login, and the will sent when the tab closed). An operator's relay
has been run on one machine, on loopback, with the page served from it, in Microsoft Edge 156
(headless): listed by a hub and reached directly, and added by its address with no hub, each time
logging in, making or picking a character and entering the world, and sending the will when the
tab closed.
