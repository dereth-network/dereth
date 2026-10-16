# The hub

Provenance: **network specification**.

One implementation: `network/hub/` (`dereth-network-hub`): `registry.rs` (the registry),
`operators.rs` (world operators, their sessions, verification, approval and reports),
`community.rs` (the community list), `persons.rs` (persons: links, codes and person friends),
`core.rs` (the routing, with no I/O), `server.rs` (the listener), `api.rs` (the
operator API and the person routes), `challenge.rs` (the challenges verification in game sends),
`fetch.rs` (the hub's own outbound requests), `forwarded.rs` (client addresses behind a
proxy), `broker.rs` (the Discord broker's client for Discord's API), `bridge.rs` (rooms
mirrored into Discord), `rooms.rs` (which of each world's rooms carries the Global room), `dms.rs` (tells into a person's Discord and its replies back),
`interactions.rs` (the bot's `/dn` command: a person's line said in a mirror from Discord) and
`self_update.rs` (installing the hub's own releases). The challenge datagram itself is `dereth_network_protocol::challenge`. `network/hub/DEPLOY.md` is the deployment guide.

The hub is one service: the **world registry** that the network's addresses, keys and federation
flags come from, and the **router** that the worlds' links meet at. It listens on loopback unless
its operator names another address; TLS is a reverse proxy's job in front of it. A hub must not
listen anywhere else without one (the reference hub refuses to unless a trusted proxy is named or
its operator passes `--insecure-listen`), since world keys travel in its requests; and a public hub
needs a proxy that holds each request until its head has arrived. A hub listening with
`--insecure-listen` and no such proxy also gives up that protection: a few addresses opening
connections whose heads never finish can then hold every connection slot for a while (the limits
below only make it slower and costlier).

## 1. The registry

- **A world** has a slug (2–40 of `a-z`, `0-9` and inner hyphens, not one of the platform's words
  below), a name (2–80 characters, no angle bracket), a description, a ruleset (`PvE`, `PvP`,
  `PvP-lite`, `Custom`), a region, a maturity, optional website and Discord links, its game endpoint (a fixed address and port, or the address
  its heartbeats come from), its emulator, and a federation flag ([01](01-addresses-and-settings.md)).
- **Lengths.** Every text field is bounded, and refused past its bound, never cut: a name 2–80
  characters, a description at most 2,000, a region 2–60, a maturity at most 60, an emulator family
  1–60 and its version 1–60, a website or Discord link at most 500 bytes; a heartbeat's message at
  most 240 characters, its emulator version at most 60 and its adapter's version at most 40 bytes;
  a report's details, an admin's note and a claim's evidence at most 2,000 characters.
- **Text.** No text the hub keeps, a world's every field, a heartbeat's message and versions, a
  report's details, an admin's note, the evidence of a claim, holds a hidden character: a control
  character (U+0000–U+001F, U+007F–U+009F), a line or paragraph separator, a bidirectional mark,
  embedding, override or isolate (U+061C, U+200E–U+200F, U+202A–U+202E, U+2066–U+2069), a
  zero-width or invisible formatting character (U+180E, U+200B–U+200D, U+2060–U+2064, U+FEFF,
  U+FFF9–U+FFFB), or U+FFFE or U+FFFF. A description, details, a note and evidence may hold line
  feeds. Text that breaks the rule is refused (`400`, naming the field), never cleaned. What the hub
  is given rather than sent (an operator's Discord display name, a world's note of who verified it
  in game) has those characters taken out instead. The community list the hub writes leaves out any
  character XML 1.0 forbids, whatever a listing holds.
- **Registration** makes a world and issues its key. The hub's operator registers on the command
  line, which makes a managed world at once; a world's own operator registers through the operator
  API (section 2), which makes a `pending` world that is listed nowhere public until it is verified.
  A world still `pending` 14 days after it was registered (the hub's setting `pendingDays`,
  section 2.14) is removed, and its slug is free again; its operator's log says so.
- **The hub's own names.** A world's game endpoint is never a name the hub or its site answers
  on, whoever registers it (`400`): the hub's domain itself (`--domain`, `dereth.network` by
  default), `hub.<domain>`, `api.<domain>`, and the hosts of the hub's public URL (`--public-url`)
  and of each site origin (`--site-origin`). Any other name under the domain belongs to the hub's
  operator: the hub's operator may register a world there, or move one there, on the command line,
  and so may an admin through the operator API (section 2.4); every other operator is refused
  (`400`), and a community listing there is a conflict (below). The command line takes
  `--public-url` and `--site-origin` (or `DERETH_HUB_PUBLIC_URL` and `DERETH_HUB_SITE_ORIGINS`)
  as `serve` does.
- **The community list** (`Servers.xml`) is imported as **unclaimed** listings: a dry run unless
  applied; an applied import with any conflict changes nothing, unless it skips conflicts
  (below); a managed listing is never
  overwritten; a slug held by another identity is a conflict, never merged. A listing becomes
  managed when the hub's operator records the ownership evidence they reviewed (`claim` on the
  command line), which issues its first key, or when a world operator's claim of it is verified
  (section 2.4). A document type or entity declaration refuses the whole document. An imported
  description's line breaks become line feeds and its tabs spaces; a record with any other hidden
  character, typed or as a character reference, is listed as refused, not imported.
  - **Reading it.** The list is read from a file, or fetched (`--fetch`) from the community
    repository's `Servers.xml` (`https://raw.githubusercontent.com/acresources/serverslist/master/Servers.xml`,
    or `--url`): a `200` answer from a public address, at most 2 MiB, within 30 seconds, through at
    most three redirects. A list that does not arrive, does not read, or holds no records changes
    nothing, and the command fails saying so.
  - **Partial apply.** With `--skip-conflicts`, an applied import applies everything but the
    conflicts and the refused records, and names them; without it, any conflict or refused record
    changes nothing.
  - **Worlds gone from the list.** Each applied import that reads counts, for every unclaimed
    imported listing the list no longer holds, the imports in a row it has been missing from. At
    `--retire-after` of them (3; 0 never) the listing is unlisted, exactly as `unlist` does, and its
    log's `unlisted` event gives the reason; it is never removed. When the list holds it again it
    is relisted (a `relisted` event), and a listing that returns before then starts its count
    again. A refused record the list still holds is not missing. A managed listing is never
    counted, unlisted or relisted, nor is a listing the hub's operator unlisted; one the operator
    relists by hand starts its count again. An update from the list keeps the operator's unlisting
    or lock.
  - **Beside a serving hub.** An applied import takes the registry's write lock at its start and
    holds it only while it writes, so it and the hub's own writes wait for each other rather than
    fail; the public lists show its changes within their one-second snapshot (and the ten seconds a
    cache may keep them).
  - **Its report.** `--summary` prints one line: added, updated, unchanged, conflicts, unlisted,
    relisted, then managed, missing and refused, naming the conflicting, unlisted and relisted
    slugs and the refused records.
- **Keys** are `drth_` and 43 characters of base64url (32 random bytes). Only a key's SHA-256 is
  kept; a key is shown once, at registration, claim or rotation. A presented key is compared by
  digest in constant time, and a wrong key, a revoked key and an unknown world take the same time.
  Rotating issues a new key and stops the old; revoking stops it until a new one is issued. Either
  closes, within a second, every link opened with the old key.
- **Heartbeats** (`POST /v1/servers/<slug>/heartbeat` with the key, or `status` over a link) keep a
  world's status fresh for 180 seconds; past that its state is `stale` and no player count shows.
  An offline world cannot report players online. An unclaimed listing or a `pending` world cannot
  report.
- **The federation flag** starts unset. The hub's operator sets it on the command line; a world's
  own operator turns it off, or on within the approval rules of section 2.6, and verifying a world
  turns it on under the same rules. `on` needs a managed world whose slug is a network slug and that
  is not locked. A world whose flag leaves `on` while linked loses its link.
- **The audit log** keeps each world's registry events (registered, claimed, verified, key issued,
  rotated or revoked, endpoint changed, federation set, imported, reported, locked), each with who
  did it: an operator, the hub itself, or the hub's operator.

### The HTTP API

| route | answers |
|---|---|
| `GET /healthz` | `{"status":"ok"}` |
| `GET /v1/config` | the registry's domain and heartbeat lifetime |
| `GET /v1/servers?q=&ruleset=&federation=&limit=&offset=` | a page of worlds: reporting online first, then by players, then by slug; `federation=on` (or `off`, `unset`) keeps only the worlds with that flag |
| `GET /v1/servers/<slug>`, `/v1/servers/<slug>/status` | one world, or its status |
| `GET /v1/resolve/<slug or slug.domain>` | the world with its endpoint resolved to addresses, as the hub already knows them: it never waits on a lookup (section 2.5), and gives only addresses on the public internet (none left answers `503` `DNS_RESOLUTION_FAILED`) |
| `GET /v1/status` | the network's totals |
| `GET /v1/network` | the federated worlds and which have a link up |
| `POST /v1/servers/<slug>/heartbeat` | records a heartbeat (key required) |
| `GET /compat/thwarg/Servers.xml` | the registry as a community list, for launchers that read that format |
| `GET /compat/treestats/player_counts-latest.json` | population counts in the shape character-statistics sites read |
| `GET /v1/servers/<slug>/link` (upgrade) | a world's link ([02](02-link-protocol.md)) |
| `POST /v1/servers/<slug>/verify` | the world's half of verification in game: the hub challenges the world's game port and holds the call for its answer (key required; section 2.5) |
| `POST /v1/servers/<slug>/verify/answer` | the world's answer to that challenge (key required; section 2.5) |
| `/v1/auth/...`, `/v1/operator...`, `/v1/reports`, `/v1/admin/...` | the operator API (section 2) |
| `/v1/person...` | a person's own routes: its linked characters, linking one, its person friends (section 5) |
| `GET /v1/directory/...` | the character directory: a search, a character, an allegiance's listed members (section 6) |
| `POST /v1/discord/interactions` | Discord's interactions: the bot's `/dn` command, signed by Discord (section 5.9) |

The lists of every world (`/v1/servers`, `/v1/status`, `/v1/network`, and the two compatibility
feeds) are answered from a snapshot of the registry at most a second old, and never wait on the
registry while something else holds it (the last snapshot is answered then, however old); they
carry `Cache-Control: public, max-age=10`, so a cache in front of the hub can take a burst. The
directory's answers carry `public, max-age=60` (section 6.3). Every other answer carries
`Cache-Control: no-store`.

Each world in the public answers carries `links`: `web`, the site's list of worlds searched for
it (`<site>/worlds/?q=<slug>`, on the first `--site-origin`), and `status` and `resolve`, the
hub's own `<public URL>/v1/servers/<slug>/status` and `<public URL>/v1/resolve/<slug>`. A hub given
neither takes itself to be at `https://hub.<domain>` and its site at `https://<domain>`. A world
has no host name of its own under the domain: where players connect is its `endpoint`.

Each world in the public answers also carries `websocketUrl` and `statusUrl`: where a browser
reaches it, its WebSocket endpoint and its status document, as its link's `hello` named them
within the rule of [02](02-link-protocol.md) section 2, while that link is up; `null` while it is
down, and for a world that named none. A launcher's web page connects to `websocketUrl` itself,
in place of a relay on the player's machine. The hub keeps what each `hello` named in the world's
registry record (`websocketUrl` and `statusUrl` beside its other fields, so the registry's schema
does not change, and a hub of an earlier version reading it leaves them be), each in place of the
last, and a `hello` naming neither clears both.

`GET /v1/config` answers `{"baseDomain", "heartbeatTtlSeconds", "authEnabled", "signIn",
"broker"}`, where `authEnabled` says whether operators can sign in on this hub, `signIn` is the
route that starts it (`/v1/auth/discord`), and `broker` is `{"discordTells", "mirrors",
"speak"}`: whether the hub carries tells into persons' Discord (section 5.8), mirrors rooms into
Discord servers' channels (section 5.7), and lets persons speak in mirrors from Discord (section
5.9), all off without the broker. A `pending` world, a locked one (section 2.8) and an unlisted one
(section 2.12) are in none of the public routes: not the lists, not `Servers.xml` or the population
counts, not `/v1/servers/<slug>` or `/v1/resolve`.

Errors are `{"error":{"code","message"}}` with the status (`400`, `401`, `403`, `404`, `408`,
`409`, `411`, `413`, `422`, `429`, `431`, `503`). Every `429`, whatever limit it is, carries `Retry-After` and
`"retryAfterSeconds"` in its error, both the whole seconds until a retry can succeed; every answer
with CORS headers also carries `Access-Control-Expose-Headers: Retry-After`, so a page on another
origin can read it. Requests, and link attempts before their key is read, are limited per client
address (180 a minute by default; an IPv6 client counted by its /64, as its connections are),
but for Discord's interactions (section 5.9), which all come from Discord's own addresses: one
Discord signed is not counted, and any other request at that route is counted once its body is
read and its signature checked. Every other request is counted once its head has arrived and
before its body is read, so a refused request is answered at once (the hub then takes in what the
client is still sending, up to 64 KiB for at most a second, before it closes, so that the answer is
not lost to a reset); a refused link is logged at most once a minute. At most 2,048
connections are open at once, and at most 32 from one address that is not a trusted proxy (an IPv6
client counted by its /64, since one host is commonly given a whole /64). A request's head may take
10 seconds to arrive, or 2 while half the connections are still sending theirs; its body may take
10 seconds in all, however it trickles in, or the request is answered `408` (`TIMEOUT`) and closed.
A request's head may be 32 KiB and 128 header lines, what Cloudflare passes on. One past either is
answered `431` (`HEADERS_TOO_LARGE`), and one that does not read as HTTP `400`, then closed as
any refused request is. Nothing past its first 32 KiB is looked at, and it is logged, with why and
its client address as far as those 32 KiB give one, at most once a minute for each address. Such
an answer carries the CORS of section 2.1 only when the head's one `Origin` line was read and
names a site origin, and none otherwise.

**The client address.** A request's client address is the address it came from, unless that is a
**trusted proxy** the hub's configuration names (`--trusted-proxy`, an address or a CIDR range,
repeated). Behind one, the hub reads the header the configuration names
(`--client-address-header`):

- **`x-forwarded-for`** (the default): the addresses in `X-Forwarded-For` are read from the right,
  one entry for each proxy a request crosses (`--proxy-hops`: 1, the default, for the hub's own
  reverse proxy alone; 2 for Cloudflare's edge and then it). Each proxy appends the address it was
  reached from, so the client is the entry that many from the right, or, before it, the first from
  the right that is not a trusted proxy (a request that came round one); a header with fewer
  entries gives its leftmost. Nothing further left, which a client or a Cloudflare Worker may have
  written, is read. Each proxy on the way must be named, Cloudflare's by its published ranges.
- **`cf-connecting-ip`**: the one address in `CF-Connecting-IP`. This is safe only where nothing
  but Cloudflare can reach the proxy that hands it on (a firewall that admits only Cloudflare's
  ranges, or a Cloudflare tunnel), since anyone else could write the header.

Without a trusted proxy, forwarded headers are ignored. From one, a header that does not read as
addresses is refused (`400`), a link attempt too, never taken to mean the proxy itself. The client
address is what the per-address allowance counts, what an observed endpoint records (a public
address only: never loopback, private, documentation or otherwise reserved), and where
verification in game challenges a world whose endpoint is observed.

The platform's words, which no world may take as its slug: `www`, `api`, `auth`, `status`,
`admin`, `account`, `accounts`, `login`, `logout`, `register`, `servers`, `characters`, `profiles`,
`docs`, `developer`, `developers`, `assets`, `static`, `cdn`, `media`, `images`, `files`,
`downloads`, `support`, `help`, `about`, `legal`, `privacy`, `terms`, `blog`, `news`, `forum`,
`forums`, `mail`, `email`, `smtp`, `imap`, `pop`, `autodiscover`, `autoconfig`, `webmail`, `ns1`,
`ns2`, `dns`, `ftp`, `ssh`, `vpn`, `metrics`, `monitor`, `health`, `staging`, `stage`, `test`,
`dev`, `localhost`, `internal`, `origin`, `tunnel`, `game`, `play`, `connect`, `oidc`, `oauth`,
`sso`, `id`, `identity`, `console`, and `discord`, which is kept for Discord itself (section
5.7).

## 2. World operators

A world's operator manages it through the hub's **operator API**: signing in with Discord,
registering a world or claiming a community listing, proving they run it, turning federation on,
rotating its key, and reporting a world that abuses the network. A player signs in the same way to
link characters (section 5): every account is a person, and an operator is a person who also
registers or claims worlds. The human-facing pages are a
separate site that calls this API from the browser; the hub serves no pages of its own. The hub's
operator keeps the command line for everything, and reviews reports there or through the admin
routes below.

### 2.1 Where the hub and the site live

The reference deployment puts the hub at **`https://hub.dereth.network`** (the API and the worlds'
links, `wss://hub.dereth.network/v1/servers/<slug>/link`) and the site at
**`https://dereth.network`**. The two are different origins on the same site, which decides how
sessions work:

- **The session cookie is the hub's.** It is set by the hub, for the hub's host only, as
  `__Host-dn_session` with `Secure`, `HttpOnly`, `SameSite=Lax` and `Path=/`. The site's scripts
  never see it. A browser sends it on the site's `fetch(..., {credentials: "include"})` calls to the
  hub, since both are on the same site (`SameSite` restricts only requests from other sites).
- **CORS.** The hub answers the operator routes (`/v1/auth/...`, `/v1/operator...`, `/v1/reports`,
  `/v1/admin/...`) and a person's (`/v1/person...`) for the origins it is configured with
  (`--site-origin`) only:
  `Access-Control-Allow-Origin` is that origin exactly, with `Access-Control-Allow-Credentials:
  true` and `Vary: Origin`. A preflight (`OPTIONS`) from one of them is answered `204` with
  `Access-Control-Allow-Methods: GET, POST, PUT, DELETE`, `Access-Control-Allow-Headers:
  Content-Type` and `Access-Control-Max-Age: 600`; from any other origin `403`, without the
  headers. The public routes of section 1 keep `Access-Control-Allow-Origin: *` and never read the
  cookie.
- **Requests that change something** (`POST`, `PUT`, `DELETE` on a session route) must carry an
  `Origin` header naming a configured site origin or the hub's own public origin, or they are
  refused `403` before the session is read. `SameSite=Lax` keeps other sites' forms from carrying
  the cookie; the origin check also refuses the site's sibling hosts (another `*.dereth.network`),
  which count as the same site. A body is JSON, at most 64 KiB, sent as `application/json` exactly
  (parameters such as `; charset=utf-8` allowed; `application/jsonp`, or a type that only mentions
  it, is refused with `400`). No request's body may be chunked: one sent with
  `Transfer-Encoding` is answered `411` (`LENGTH_REQUIRED`) and its connection closed.
- **The page the hub links back to** after sign-in is on the first configured site origin.

A deployment that serves the hub under the site's own origin instead (`https://dereth.network/hub`,
the proxy stripping the prefix) needs no CORS; it sets `--public-url` to the prefixed address, and
the same cookie then reaches every path of the site, which holds only static pages.

### 2.2 Signing in

Sign-in is Discord's OAuth 2 authorisation-code flow, run by the hub, which holds the Discord
application's client secret. The hub asks for the `identify` scope only and keeps, per operator,
the Discord user id, the user name and the display name, and when they signed in; it keeps no
Discord token.

| route | does |
|---|---|
| `GET /v1/auth/discord?return=<path>&site=<origin>` | Starts sign-in: `302` to Discord's authorisation page, with a fresh `state`, and the cookie `__Host-dn_oauth` (`Secure`, `HttpOnly`, `SameSite=Lax`, ten minutes) that carries the sign-in: the state, when it lapses and where to return, signed by the hub with a key it makes when it starts. The hub keeps nothing of a sign-in in progress, so no number of them started can push another out. `return` is a path on the site to come back to (`/` unless it is a path of at most 200 characters that starts with one `/`). `site` is the site origin the sign-in was started from; it is kept only when it is exactly one of the hub's site origins (`--site-origin`), and otherwise the first of them is used, so no other address can become a place a sign-in returns to. "The site" below means that origin. While Discord asks the hub to wait (below), it answers `303` to the site at `return` with `signin=busy` instead, and starts nothing. |
| `GET /v1/auth/discord/callback?code=&state=` | Discord sends the browser here. The cookie must be one the hub signed, within its ten minutes, and its state the query's, used once. The hub exchanges the code, reads the user, records the operator, starts a session, sets `__Host-dn_session`, clears `__Host-dn_oauth`, and answers `303` to the site at `return`. Any failure (a refusal at Discord, a wrong, altered, lapsed or spent sign-in, Discord unreachable) answers `303` to the site at `return` with `signin=failed` added to its query, and starts nothing; while Discord asks the hub to wait (below), `signin=busy` instead, without calling Discord. |
| `POST /v1/auth/signout` | Ends the session: `204`, and the cookie is cleared. |
| `GET /v1/operator` | The operator signed in, or `401`. |

```json
{"operator": {"id": "op_5f0c9a2e41d7b3c8", "discordId": "80351110224678912", "name": "Nelly",
              "signedInAt": "2026-10-11T18:02:11.000Z", "admin": false}}
```

**When Discord answers `429`**, the hub calls it for nothing until the wait it names is over: its
`Retry-After`, or else its body's `retry_after`, in whole seconds rounded up, at least one and at
most an hour, and a minute when it names neither. The wait is the hub's as a whole, for every
sign-in. Until it is over, a sign-in started or coming back answers `signin=busy`, for the site to
say to try again shortly; the next sign-in after it calls Discord again.

A session lasts 30 days from sign-in, or until sign-out. Its id is 32 random bytes; the hub keeps
only its SHA-256. A session whose operator the hub's operator has disabled (section 2.12) ends at
once. `admin` is true for an operator whose Discord id the hub's configuration names as one of its
admins.

The hub is configured with the Discord application's client id (`--discord-client-id`) and the
client secret, which is read from the environment variable `DERETH_HUB_DISCORD_CLIENT_SECRET` or a
file named by `--discord-client-secret-file`, never from a command-line argument or a repository.
Discord's redirect URI is `<public URL>/v1/auth/discord/callback` (`--public-url`). A hub without
them answers every operator route `503` and works otherwise as before. The application's bot
token, which turns the Discord broker on (section 5.6), is read the same way, from
`DERETH_HUB_DISCORD_BOT_TOKEN` or a file named by `--discord-bot-token-file`; a secret is never
written to the log, nor is any argument the command line does not take, which a secret given there
by mistake would be.

### 2.3 A world as its operator sees it

The operator routes show a world as section 1's routes do, with these fields added. (Section 1's
`endpoint` is `{"mode", "address", "port", "addressFamily", ...}`: `addressFamily` is the observed
endpoint's `ipv4` or `ipv6`, and `null` for a fixed one, so a world read and written back keeps
its endpoint.)

```json
{
  "slug": "coldeve", "name": "Coldeve", "...": "every public field",
  "ownership": {"state": "managed", "verifiedAt": "2026-10-11T18:20:40.000Z"},
  "operator": {
    "role": "owner",
    "verification": {"method": "game", "verifiedAt": "2026-10-11T18:20:40.000Z",
                     "endpointVerified": true, "open": false, "expiresAt": null},
    "key": {"issuedAt": "2026-10-11T18:05:02.000Z"},
    "locked": null
  }
}
```

- **`ownership.state`** is `pending` (registered here, not yet verified; listed nowhere public),
  `managed` (verified, or registered or claimed by the hub's operator), or `unclaimed` (a community
  listing). **`ownership.endpointVerified`**, in every view, public ones included, is whether the
  world's current endpoint is the one it was verified at (false for an unclaimed listing). `operator.role` is `owner` for the operator the world belongs to, and `claimant` for an
  operator with an open claim on an unclaimed listing.
- **`operator.verification`**: how and when the world was last verified (`page`, `game`,
  `review` for the hub operator's own registration or claim, or `admin` when the hub's operator
  vouched for it, section 2.5); `endpointVerified`, false once the endpoint has changed since;
  and whether a verification is open, until when.
- **`operator.locked`**: `{"at", "reason"}` once the hub's operator has revoked the world
  (section 2.8); a locked world is in no public answer, and its operator can neither edit it
  (`409` `locked`), turn federation on nor give it a key.

### 2.4 Registering and claiming

| route | does |
|---|---|
| `GET /v1/operator/worlds` | `{"worlds": [...]}`: the operator's worlds and open claims. |
| `POST /v1/operator/worlds` | Registers a world (the body below): `201` with the world (`pending`), its key and a verification. `409` when the slug is taken. |
| `GET /v1/operator/worlds/<slug>` | `{"world": ..., "audit": [...]}`: one of the operator's worlds and its last 50 registry events, newest first (section 2.10). A claimant sees only the events it caused, never another claimant's; the owner sees every event, another operator's without its id. |
| `PUT /v1/operator/worlds/<slug>` | Replaces the registration (the same body; `slug` must be the path's): `{"world": ...}`. A changed endpoint is no longer the one the world was verified at: `endpointVerified` is false, federation is turned off, and it cannot be turned on again (`409` `endpoint_not_verified`) until the world is verified at the new endpoint (a new token, then section 2.5), which turns it on again on the terms of section 2.6. The world stays listed, its public `ownership.endpointVerified` false meanwhile. |
| `DELETE /v1/operator/worlds/<slug>` | Withdraws: a `pending` world is removed, a claim is closed. `204`. A managed world is kept (`409`): the hub's operator removes one. |
| `POST /v1/operator/worlds/<slug>/claim` | Claims an unclaimed listing: `201` with the listing, a key and a verification. The listing stays as it is, and public, until the claim is verified. Another operator's `pending` world, and a locked or unlisted listing, are not found (`404`). |

The registration body is the same as the command line's `register` file:

```json
{
  "slug": "coldeve", "name": "Coldeve", "description": "A classic PvE world.",
  "ruleset": "PvE", "region": "US East", "maturity": "Unspecified",
  "website": "https://coldeve.example.org", "discord": "https://discord.gg/example",
  "isDemo": false,
  "endpoint": {"mode": "manual", "address": "play.coldeve.example.org", "port": 9000},
  "emulator": {"family": "ACE", "version": "1.77"}
}
```

- **The operator's Discord account** must be at least 30 days old (the hub's setting
  `minAccountAgeDays`, section 2.14), by the time its id says it was made, to register a world or
  claim a listing (`403`, `reason` `account_too_new`), so that accounts made by the dozen cannot.
- **The slug** follows section 1's rules (2 to 40 of `a-z`, `0-9` and inner hyphens, not a platform
  word), and only a slug of at most 12 that is not one of the network's reserved names
  ([01](01-addresses-and-settings.md)) can join the network; a registration with a longer one is
  accepted and stays off it. The other fields follow section 1. An endpoint given as an address
  must be a public one (not loopback, private, link-local or reserved), and one given as a host
  name must not resolve to any other kind (a name that does not resolve yet, or not within the
  lookup deadline, is taken, and cannot be verified until it does); the same holds when an
  endpoint is changed. Only an admin's endpoint may be a name under the hub's domain, and nobody's
  one of the hub's own names (section 1). `"mode": "observed"` (the address the world's heartbeats come from) is
  allowed too.
- **The key** is returned once, with the registration or the claim (`"key": "drth_..."`), and kept
  as its digest. It works only for the world's verification call and its answer (section 2.5)
  until the world is verified; then it is the world's key, and the world can link once federation is on. A claim's key
  replaces the listing's (none) only when the claim is verified; a claim that is withdrawn, or
  overtaken by another operator's verified claim, takes its key with it.
- **The verification** returned beside them is section 2.5's.
- **A world the hub's operator registered** on the command line belongs to no operator until they
  hand it to one (`dereth-network-hub assign <slug> <discord id>`); it is then the operator's like
  any other, verified by review.

```json
{"world": {"slug": "coldeve", "...": "..."}, "key": "drth_Ue3...",
 "verification": {"token": "dnv_q0Zx4l...", "expiresAt": "2026-10-18T18:05:02.000Z",
                  "page": {"hosts": ["play.coldeve.example.org", "coldeve.example.org"]},
                  "game": "@dn verify dnv_q0Zx4l..."}}
```

### 2.5 Proving the operator runs the world

A verification is a one-time **token**, `dnv_` and 32 characters of base64url (24 random bytes),
valid for seven days and for one world; the hub keeps only its SHA-256. A new one
(`POST /v1/operator/worlds/<slug>/verification`, answering `{"verification": ...}`) replaces the
last. The world is verified by either of these, after which the token is spent:

**A page on the world's own host.** The operator puts the token anywhere in a page on the host
the world's endpoint names, and asks the hub to read it:

| route | does |
|---|---|
| `POST /v1/operator/worlds/<slug>/verify` `{"url": "https://coldeve.example.org/network.txt"}` | The hub fetches the page and looks for the token. `200` `{"world": ..., "approval": ...}` once verified; `422` when not, with the reason. |

- **The URL** is `http://` or `https://`, without credentials, on port 80 or 443, and its host
  is the endpoint's own: the same host name (never a domain it is under), or, for an endpoint
  given as an address, that address. A world whose endpoint is observed is verified in
  game only. Its host is ASCII (an internationalised name in its `xn--` form); anything in its
  path or query outside printable ASCII is sent percent-encoded, and so is a redirect's. A URL
  that does not read as one is refused (`400`).
- **What the hub fetches.** It resolves the host and refuses the page unless every address it
  resolves to is public: not loopback, private (RFC 1918, IPv6 unique-local), link-local, shared
  (`100.64.0.0/10`), multicast, documentation, benchmarking or otherwise reserved, an IPv4 address
  mapped into IPv6 or translated (`64:ff9b::/96`) from one of those, or unspecified. It connects to
  an address it checked, never resolving the name again, with TLS for `https://` (the name checked
  against the certificate). It sends one `GET`, follows at most three redirects, each to a URL that
  passes every one of these rules (so a redirect to another host, or into a private range, is
  refused), takes only a `200`, reads at most 64 KiB of the body, and gives up after 10 seconds in
  all. The token must appear in what it read. It never connects to a range its operator denies
  (`--deny-fetch`: the hub's own public address, its prefix and its local network), whatever else
  allows it.
- **What it says.** A URL that is not on the world's host is refused before anything is read
  (`host_mismatch`). Once the hub reads, every failure is one answer, `token_not_found` ("The page
  did not show the token"), whether the page lacked the token, answered other than `200`,
  redirected where the hub does not follow, could not be reached, or took too long: the hub tells
  nobody what it reached or what answered. Its own log says which, for its operator.

**A command in game.** An administrator of the world types `@dn verify <token>` (`/dn verify` works
too, as for every command; at the server console it is `dn verify <token>`). The world sends the
hub the token with its key, from whatever address its calls leave by; the hub sends a challenge to
the world's game port, and the world sends the challenge's nonce back
([03](03-world-behaviour.md) section 6 is the world's half):

| route | does |
|---|---|
| `POST /v1/servers/<slug>/verify`, `Authorization: Bearer drth_...` `{"token": "dnv_...", "by": "Asheron"}` | Checks the key, the token and the attempts, challenges the world's endpoint, and holds the call until the world answers or the hub stops waiting: `200` `{"verified": true, "federation": "on", "approval": "approved"}` once verified; `401` for a key that is not the registration's or claim's; `409` (`in_progress`) while another verification of the world waits for its answer; `422` when not verified, with the reason; `429` past the attempts. |
| `POST /v1/servers/<slug>/verify/answer`, `Authorization: Bearer drth_...` `{"nonce": "<32 hex digits>"}` | The world's answer: `200` `{"answered": true}` when the nonce is one the challenge waiting for this world sent and the key is the one its verification call came with; `422` (`wrong_nonce`) otherwise, which leaves the challenge waiting; `401` for a key that is not the world's or a claim's. |

- **The key and the token.** The key must be the one the registration or the claim returned (or,
  for a world verifying again, its current key), and the token that world's open token. The key is
  checked before anything else: nothing is looked up, counted or sent for a call without one.
  The attempt is counted once the key is the world's (section 2.9), before anything is sent.
- **Where the hub challenges.** A host name is looked up, waiting at most ten seconds (longer than
  for anything else, since verification is rare and a name server may be slow); an address is
  itself; an observed endpoint is the call's own client address (section 1's, behind a trusted
  proxy). Only an address the hub may reach is challenged, under the rules a page fetch is held
  to: an address on the public internet, outside every range `--deny-fetch` names. At most the
  first four such addresses are challenged, at the endpoint's game port. The call itself may come
  from any address: a world behind two connections to the internet, a load balancer or a VPN's
  exit is verified alike.
- **The challenge.** Each address challenged is sent its own fresh random nonce, 16 bytes, three
  times, two seconds apart, since UDP may lose any one: at most twelve datagrams for an attempt,
  each smaller than the request that caused it, and only after a call with the world's key, its
  open token and an attempt left. At most one challenge waits for a world at a time, and at most
  256 across the hub (past that, `429`).
- **The answer.** A world that asked the hub to verify it in the last minute reads the challenge
  on its game port and posts its nonce back on the answer route with the same key, over HTTPS,
  from any address. It never answers over UDP, so the datagram draws nothing back to wherever its
  source says it came from, and a world with no verification waiting ignores it. The hub holds the
  verification call up to 15 seconds from its first datagram; the first nonce that comes back
  verifies the world at the address that nonce was sent to, which the audit log records. An
  observed endpoint's address becomes the one the world answered at.
- **Once answered**, the verification must still be the one the call checked, with the same token,
  not expired, at the same endpoint; otherwise it fails with the matching reason below, and the
  world is not verified.
- `by` names who typed it (at most 32 characters), for the audit log.

Every message names the host, the addresses and the port involved:

| reason | message |
|---|---|
| `no_answer` | *The hub's challenge to retail.dereth.network:19000 (99.113.32.94) got no answer: is the game port reachable from the internet?*; for an endpoint given as an address, *The hub's challenge to 99.113.32.94:19000 got no answer: ...*; for an observed one, *The hub's challenge to 99.113.32.94:19000, the address this call came from, got no answer: ...* |
| `not_resolved` | *retail.dereth.network did not resolve in time: try again in a minute*, or *retail.dereth.network does not resolve to any address: check its DNS records* |
| `address_not_public` | *retail.dereth.network resolves to no address the hub may reach on the public internet (10.0.0.5)*; *10.0.0.5 is not an address the hub may reach on the public internet*; *The call came from 10.0.0.5, which is not an address on the public internet* |

**The challenge datagram** is shaped as the status ping is
([networking/06](../networking/06-status-ping.md)): a 20-byte header in the shape of a packet
header, whose size field is larger than any datagram a server reads, so that ACE, GDLE, and any
world that does not know the challenge (or has the network off) drop it at their packet reader's
size check, before they read anything else. Its magic is its own.

| bytes | value |
|---|---|
| 0–3 | `DNVC` (`44 4E 56 43`), in place of the sequence number |
| 4–15 | zero: no flags, no checksum, recipient 0, time 0 |
| 16–17 | `FF FF`: the payload size, larger than any datagram can carry |
| 18–19 | zero |
| 20 | `01`: a challenge |
| 21 | the version, `01`; a later version only appends fields, and a reader ignores what follows the ones it knows |
| 22–37 | the nonce, 16 random bytes |
| 38 | the slug's length *n*, 1 to 40 |
| 39–(38 + *n*) | the world's slug, `a-z`, `0-9` and `-` |

So a challenge is at most 79 bytes. A world ignores one whose slug is not its own. The nonce goes
back as 32 hex digits; the conformance vectors pin the bytes ([06](06-conformance.md)).

**The hub's operator vouches.** When a world cannot be verified either way (a game port the hub
cannot reach, a host that serves no page), the hub's operator may vouch for it on the command line,
having reviewed evidence that the world's operator runs it at its endpoint:
`dereth-network-hub verify <slug> "<evidence>"`. It verifies the world as a successful proof does,
by the method `admin`: a `pending` world becomes `managed`, its endpoint counts as verified, its
owner's open token is spent, and approval is asked for its owner on the terms of section 2.6 (a
world with no owner keeps its federation flag). The evidence, 10 to 2,000 characters with no hidden
character, as `claim`'s, goes in the world's log as a `verified` event of the hub's operator's
(`hub-operator`, naming no operator). It refuses what `claim` refuses: a world that is not there
(removed, or lapsed while `pending`) and a locked one; and a community listing, which `claim`
takes.

**Lookups.** The hub looks a host name up on a worker of its own, at most eight at once, and waits
at most two seconds for one (ten for verification in game); what a lookup finds is used for a
minute, and one that ends after its caller stopped waiting is still kept for the next. A call that
finds every worker busy, or waits past the deadline, has no addresses: a verification call then
answers `not_resolved`. `/v1/resolve` never waits: for a host name the hub has not looked up in the
last minute it answers `200` with `resolution.source` `unknown`, no addresses and the endpoint's
host name as configured, and starts a lookup, so that asking again in a moment
(`refreshAfterSeconds`, 2) has the addresses.

Every attempt, either way, counts against the world's attempts (section 2.9), and its outcome goes
in the audit log. A failure answers:

```json
{"error": {"code": "VERIFICATION_FAILED", "reason": "token_not_found",
           "message": "The token was not on the page."}}
```

with `reason` one of `token_not_found` (by a page: whatever happened once the hub read), `host_mismatch`
(the URL is not on the world's host), `wrong_token` (not the world's open token, or spent, or
replaced by a new one while the challenge waited), `expired`, `no_answer` (the challenge got no
answer in time), `not_resolved` (the endpoint's host name did not resolve), `address_not_public`
(no address to challenge is one the hub may reach), `endpoint_changed` (the world's endpoint
changed while the page was read or the challenge waited: a verification counts only for the
endpoint it checked) and `no_verification` (no token is open, or the verification was closed while
the challenge waited). The audit log records the same reasons.

**What verification proves, and what it does not.** Verification shows that whoever asks holds the
world's key (or its claim's) and controls something at the world's endpoint: a page its host serves
on the web's own ports, or a program that receives what is sent to the endpoint's game port. It
proves no more than that, and it does not ask where the world's own calls leave from. On a host
several people share (a web host whose users can each publish pages under its name, a machine
with several accounts, any of which could listen on the game port while the world is down), any
of them can verify a world at that endpoint; behind an address several people share (a household
or carrier-grade NAT), whoever that game port is forwarded to can. The hub bounds what that allows
rather than preventing it: a verification counts only for the registration or claim it was opened
for and only for the endpoint it checked, a changed endpoint must be verified again, each
operator's allowances bound what one account can do, and a report lets the hub's operator revoke
and lock a world that passes itself off as another. A vouch proves what its evidence showed the
hub's operator, and keeps that evidence in the world's log.

### 2.6 Approval

Verifying a `pending` world makes it `managed`; verifying a claim makes the listing `managed`, the
claimant's, with the claim's key, and closes every other claim on it. Then federation is turned on
by itself when the world's slug is a network slug, the world is not locked, and the approval
allowances are not spent (three worlds a day for one operator, and the hub's daily ceiling for all
operators, 500 by default, set above what operators' own allowances reach together so that a few
accounts cannot spend it for everyone; the ceiling is the hub's setting `approvalsPerDay`, section
2.14). The answer's `approval` says which:

| `approval` | federation |
|---|---|
| `approved` | turned `on` |
| `not_network_slug` | stays as it was: the slug cannot join the network |
| `locked` | stays as it was |
| `unlisted` | stays as it was: the hub's operator took the world out of the lists |
| `rate_limited` | stays as it was; ask again later |

| route | does |
|---|---|
| `PUT /v1/operator/worlds/<slug>/federation` `{"flag": "on"}` | Turns federation on for a verified world, on the same terms: `{"federation": "on", "approval": "approved"}`; or `409` with `error.reason` `not_verified` (the world is `pending`), `endpoint_not_verified` (its endpoint changed since it was verified), `not_network_slug`, `locked` or `unlisted`; or `429` with `error.reason` `rate_limited`. |
| `PUT /v1/operator/worlds/<slug>/federation` `{"flag": "off"}` | Turns it off: always allowed, and the world's link closes. |

### 2.7 Keys

| route | does |
|---|---|
| `POST /v1/operator/worlds/<slug>/key` | A new key for a verified world: `{"key": "drth_..."}`, shown once. The old key stops at once, and every link opened with it closes within a second. |

### 2.8 Reports, and revoking a world

Any signed-in operator may report a world. Reports are about worlds only, and need a sign-in: a
character's conduct is its own world's staff's business, by the world's own rules.

| route | does |
|---|---|
| `POST /v1/reports` `{"slug": "coldeve", "reason": "impersonation", "details": "..."}` | `201` `{"report": {"id", "slug", "reason", "state": "open", "createdAt"}}`. `reason` is `impersonation`, `abuse`, `spam` or `other`; `details` is at most 2,000 characters. One open report per operator per world. |
| `GET /v1/operator/reports` | `{"reports": [...]}`: the operator's own reports and how each ended. |

The hub's operator reviews them on the command line (`dereth-network-hub reports`,
`dereth-network-hub report <id> dismiss|revoke "<note>"`), or as an admin through:

| route | does |
|---|---|
| `GET /v1/admin/reports?state=open` | `{"reports": [...]}`, each with the reporter and the world (`state=all` for every report). |
| `POST /v1/admin/reports/<id>` `{"action": "dismiss", "note": "..."}` | Closes the report. |
| `POST /v1/admin/reports/<id>` `{"action": "revoke", "note": "..."}` | Closes it and **revokes the world**: federation `off`, its key revoked, open claims closed, and the world locked, so its operator can neither turn federation on nor issue a key, and a locked listing cannot be claimed (an operator's claim is not found; the command line's `claim` and `rotate-key` refuse it too). The hub's operator lifts a lock with `dereth-network-hub unlock <slug>`. |

A report reads:

```json
{"id": "rp_4c1f0a9e2b7d3586", "slug": "rogue", "reason": "impersonation",
 "details": "Not the real Rogue.", "createdAt": "2026-10-12T09:14:03.000Z",
 "state": "closed", "resolvedAt": "2026-10-12T11:40:52.000Z", "resolution": "revoked",
 "world": {"slug": "rogue", "name": "Rogue"},
 "note": "Impersonation.",
 "reporter": {"id": "op_9a7e33b0c2d14f68", "discordId": "80351110224678912", "name": "Nelly"}}
```

`state` is `open` or `closed`; a closed report's `resolution` is `dismissed` or `revoked`, and
`resolvedAt` says when (both `null` while open). `note` is the admin's, and `reporter` who filed
it: both are in the admin routes' answers only. An operator's own reports
(`GET /v1/operator/reports`) carry neither. An admin route answers `403` to an operator who is not
an admin, and the reporter is never shown to the reported world's operator: its log says only that
the world was reported, and why. A reporter that is not an operator and forgets itself (section 5.3)
takes its reports with it, open or closed; the world's log, and a revoke made on one, stay.

### 2.9 Limits

Every request but Discord's signed interactions takes from its client address's allowance (section
1). The operator routes are also
held to these, each answered `429` with `Retry-After` past it:

| what | limit |
|---|---|
| registrations | 3 a day for one operator, and at most 10 worlds and claims held at once |
| open claims | 3 for one operator, and 5 claims opened a day, withdrawn or not |
| new verification tokens | 10 a day for one operator's verification of one world |
| verification attempts (page or game) | 10 an hour for one operator's verification of one world, counted once the key is the claimant's |
| page fetches | 30 an hour for one operator, and a ceiling of 3,000 an hour for the hub; a page the hub fetches, never a URL refused before (not on the world's host) |
| federation approvals | 3 a day for one operator; the hub's daily ceiling for all (500 by default, `approvalsPerDay`, section 2.14) |
| new keys | 10 a day for one world |
| registration changes | 30 a day for one world |
| reports | 5 a day for one operator |

An admin (section 2.2) is held to none of one operator's own limits here, only to the hub's ceilings
and to reports; its client address's allowance, and the limits of sections 5 and 6, hold for it as
for everyone.

The counts are taken from the audit log, so a restart does not reset them, and neither does an
account forgetting itself (section 5.3) and signing in again. A count for one world is each
operator's own: one claimant's attempts and tokens never spend another's.

```json
{"error": {"code": "RATE_LIMITED", "message": "Too many registrations today", "retryAfterSeconds": 41230}}
```

### 2.10 The audit log

An entry reads `{"kind", "at", "actor", "details"}`: what happened, when (RFC 3339 UTC), who did it
(an operator's id; `hub` for the hub itself, approval included; `world` for the world's own
verification call; `hub-operator` for the hub's operator on the command line), and what the event
records, an object whose fields depend on `kind`. A world's owner reads its own id, and never
another operator's: an operator that registered, claimed or was handed the world before it is
`earlier-claimant` (in `actor`, and in an `assigned` event's `operator`), and any other, an admin
resolving a report, is `hub-admin`. The hub's command line (`audit`) names every operator by id,
but for an account since forgotten (section 5.3), which every reader sees as the world's owner does:

```json
{"kind": "verified", "at": "2026-10-11T18:20:40.000Z", "actor": "op_5f0c9a2e41d7b3c8",
 "details": {"method": "game", "by": "Asheron", "address": "99.113.32.94"}}
```

Every operator action is a registry event of the world it acts on, with the operator who did it:
registered, claimed, claim withdrawn, verification issued, verification attempted (its method and
outcome), verified, endpoint changed, federation set (and by whom: an operator, the approval, the
hub's operator), key issued, rotated or revoked, reported, report resolved, locked and unlocked,
a room of the world's mirrored into Discord or no longer (`discord-mirror`, section 5.7), by its
operator, by the admin who took it away, or by the hub as the room that carries the Global room
changed, and where the world carries the Global room (`global-room`, section 5.10), by its operator
or by the world. Sign-in and sign-out are the operator's own events.
A change to the hub's settings is an event of the hub's own settings log (section 2.14).
No key, token, session id, Discord token, bot token, client secret or fetched page is ever
written to the log or the database. The one secret the registry keeps is the webhook an operator
gives a mirror (section 5.7), the only way into its channel; it is never answered or logged.

### 2.11 Errors

Operator routes answer with section 1's error form and statuses, and `422` for a verification
that failed (`VERIFICATION_FAILED`, above). `401` (`UNAUTHORIZED`) means no session, or one that
ended; `403` (`FORBIDDEN`) a request from an origin that may not make it, a disabled operator, an
admin route for someone who is not one, or a world that is not the operator's; `404` a world that
does not exist or that the operator cannot see (another operator's `pending` world is not found);
`409` (`CONFLICT`) a slug that is taken or a change the world's state does not allow; `503`
(`UNAVAILABLE`) a hub without sign-in configured.

### 2.12 The command line

The hub's operator has, beside section 1's commands: `reports` (the open reports) and
`report <id> dismiss|revoke "<note>"`; `unlock <slug>`; `assign <slug> <discord id>`;
`verify <slug> "<evidence>"`, which vouches for a world (section 2.5); `operators`;
`end-sessions`, which ends every session (after a restore, so that sessions ended since the backup
stay ended; everyone signs in again);
`disable-operator <discord id>` and `enable-operator <discord id>`; `remove <slug>`, which removes a
world for good, whatever its state (its key, claims, reports, log and its characters' links to
persons go with it, its slug is free,
and its operator's log says so); and `unlist <slug>` and `relist <slug>`, which take a world out of
every public answer and put it back. An unlisted world's federation is turned off, and neither its
operator nor the hub's operator can turn it on until it is relisted; its operator still sees and
edits it. A disabled operator's
sessions end, and it can sign in no more until enabled; its open verifications and claims are
closed, its worlds leave the network (federation `off`), and their keys work for nothing, a link
or a heartbeat included, until it is enabled again. Enabling it gives the keys back; the network
stays off until its operator turns it on.

### 2.13 Updating the hub

`self-update --check|--apply [--policy patch|minor] [--source <url>]` installs a newer release of
the reference hub over the binary it runs from. It starts and stops nothing, and a running hub never
replaces itself.

- **Releases** are the published GitHub releases tagged `network-hub-v<version>` of the repository
  the binary was built from, or of `--source` (a GitHub repository, or a mirror of GitHub's releases
  API at `http(s)://<host>/.../repos/<owner>/<name>`). A release has, for each target it is built
  for, `dereth-network-hub-<version>-<target>.tar.gz` (`.zip` for Windows) holding
  `dereth-network-hub-<version>-<target>/dereth-network-hub` (`.exe` for Windows), and a
  `SHA256SUMS` listing every file.
- **Which release.** The newest the policy allows. `patch` takes releases of this build's major and
  minor version, and installs one only when its binary reads this build's registry schema, so that
  nothing but the binary changes. `minor` (the default) takes releases of this build's major version.
  Never taken: a major release, a pre-release (published as one, or a version with a `-` suffix), a
  release with no archive for this build's target or no `SHA256SUMS`, and a release that failed
  here. `--check` names each newer release with why it would not be installed, and the one that
  would.
- **Installing** (`--apply`) stops at the first check that fails, with the binary unchanged:
  1. before anything is downloaded, the binary's folder must be writable by this account (a refusal
     names the folder) and the registry (`--db`) must exist;
  2. the archive's SHA-256 must be the one `SHA256SUMS` lists. Releases are not signed: this check,
     by SHA-256 only, catches a damaged or altered download, not a release published by someone else;
  3. the new binary, run from the binary's folder, must print `dereth-network-hub <version>` for
     `--version`, and for `--registry-schema` a schema no older than the registry's;
  4. the registry is copied as `backup` copies it, read without being opened as a registry (nothing
     is created or moved forward), to `<registry>.backup-update-v<old>-v<new>-<UTC time>` beside it,
     owned by the registry's owner and readable by it alone; the newest three such copies are kept;
  5. the binary is moved aside as `<name>.previous-v<old>` (the one an earlier update left is
     removed) and the new one moved into its place.
- **A release that failed here**: one whose binary does not run, names another version, does not
  say its registry schema, or reads an older schema than the registry's. It is recorded beside the
  binary (`.dereth-network-hub-update.json`) and not taken again while the record stands; a
  successful update clears the record. A download that does not match its checksum, and a release
  `patch` refuses for its schema, are not recorded.
- **Exit status:** 0 when a release was installed (with `--check`, when one would be), 3 when none
  is to be, 1 when it refused or failed.
- **Going back.** A hub refuses to start on a registry whose schema is newer than the one it reads
  (`--registry-schema` names it), so the binary moved aside runs again only on a registry it reads:
  after a `patch` release always, after a `minor` one that moved the schema forward only with the
  update's copy of the registry put back.

### 2.14 The hub's settings

Three of the hub's rules are its admins' (section 2.2) to change while it runs. The registry keeps
them; a setting it holds no value of is its default.

| setting | default | bounds | what it is |
|---|---|---|---|
| `approvalsPerDay` | 500 | 0 to 100,000 | the hub's ceiling on worlds approved onto the network a day, for all operators together, above each operator's own 3 (sections 2.6 and 2.9); 0 approves none |
| `minAccountAgeDays` | 30 | 0 to 3,650 | how old an operator's Discord account must be, in days, to register a world or claim a listing (section 2.4); 0 asks no age |
| `pendingDays` | 14 | 1 to 365 | how long a registered world may stay `pending`, in days, before it is removed and its slug freed (section 1) |

| route | does |
|---|---|
| `GET /v1/admin/settings` | `{"approvalsPerDay": {"value", "default", "min", "max"}, "minAccountAgeDays": {...}, "pendingDays": {...}}`: each setting's value now, its default and its bounds. |
| `PUT /v1/admin/settings` `{"pendingDays": 21}` | Sets any of the three, each by its name: `200` and the settings, as `GET` answers them. `400`, naming the setting, for a value that is not a whole number within its bounds, and for a name that is not a setting or a body that names none; then nothing changes. |
| `GET /v1/admin/settings/log` | `{"events": [{"kind", "at", "actor", "details"}]}`: the hub's own settings log, newest first, at most 200. |

- **A change holds at once**, wherever its setting is used, with no restart: the approval the next
  verification or federation request asks for, the next registration or claim, and the next look
  at the `pending` worlds, each world counted from when it was registered. The hub's command line
  reads the settings from the registry as the running hub does.
- **Each value set** is an event of the hub's settings log, `setting`, `{"name", "value", "was"}`
  (`was` the value in effect before), by the admin who set it. Setting the value the registry holds
  already changes nothing and records nothing.
- **An operator who is not an admin** is answered `403` on each route, as on every admin route.
- **Flags at start.** The hub's operator once gave these as flags of `serve`: `--approvals-per-day`,
  `--min-account-age-days` and `--pending-days`. A flag given still seeds its setting while the
  registry holds no value of it, as an event of the hub's operator's (`hub-operator`, with
  `"flag"` in its details), and the log says so. Once the registry holds a value, that value
  holds: a flag that differs from it is ignored, and the log says so, naming both values. A flag
  outside its setting's bounds stops the hub from starting. These flags are to be removed in a
  later minor release.

## 3. Routing

The hub's rules are [02-link-protocol.md](02-link-protocol.md)'s, and these:

- **Federation.** Everything that names a world whose flag is not `on` is refused: a tell to it is
  `not_on_network`; a watch, presence, invitation or room line for it goes nowhere; it may not
  link.
- **Presence is the home world's.** The hub hands a `watch` to the target's home world, within the
  sending world's share of watches to it ([02](02-link-protocol.md) section 5), and the `presence`
  answer to the watcher's world; it decides nothing about who is visible and keeps no presence
  state.
- **Rooms.** A `room.line` goes to every other linked world that has joined the room. A world that
  has not joined a room cannot say anything in it.
- **Channels** are the hub's own: name, owner, members and invitations, saved with the registry, one
  channel a row, the channels changed since the last save written together in one transaction. A
  character may own at most 10 channels and belong to at most 50; the characters of one world may
  own at most 100 together (a world names its own characters, so the per-character limits alone hold
  only for an honest world); a channel may have at most 200 members and 200 invitations outstanding.
  Only a member may invite; an invitation is handed on to its world once (a repeat stands and is not
  sent again) and lapses after a week, and one past the per-recipient limit of tells from the
  inviter's world is refused; only an invited character may join; a member's tell to the channel
  goes to every other member whose world is linked, each line within the limits a tell from the
  speaker's world to that member is held to.
- **Frames.** Every frame the hub sends is within the link's 16 KiB: `welcome` names as many
  worlds as fit and a `world` message follows for each of the rest, a channel's member list or a
  character's channel list is cut to fit, and everything it hands on was read within the field rules
  (an away text is held to 1,024 characters like any text), under an id no longer than the one it
  replaces by more than fits.
- **A world that stops reading** is dropped: once a write to it has waited 10 seconds, or at once
  when the frames waiting for it pass 4 MiB. The other worlds are told it is offline, and its
  connection is freed.
- **Resends.** A request the hub has already taken (a tell, a room line or a channel request, by
  `(world, id)`, for 5 minutes) is not taken again; its repeat is answered with the answer the
  first was given.

## 4. Privacy and operation

- **No message bodies** are logged or written anywhere. A tell is held in memory only, at most 60
  seconds, while its target world reconnects; a tell to a linked character until its world has
  answered it and the question that follows (at most a minute), and then, should it go to the
  person's Discord, until it is said or held back (section 5.8); a line for Discord only until it is
  said there or held back (section 5.7); and a line said from Discord only until its world is asked
  to say it (section 5.9). The log names message types and worlds.
- **No presence history**, no telemetry beyond the registry's heartbeat. While it runs, the
  routing remembers in memory alone when each character its worlds watched was last online (at most
  100,000 of them, none past 30 days), to choose which person friends a view holds when a person
  lists more than a view does (section 5.4); a hub that starts again remembers none.
- **No game accounts.** The hub knows a character only as an address its home world vouches for.
  The only accounts are Discord users who signed in (section 2): a Discord id, a name and sign-in
  times, and, for a person who links characters, section 5's links and person friends.
- **Behind a proxy.** A proxy in front of the hub must pass WebSocket upgrades on, and its idle
  timeout must be longer than the link's keep-alive: the hub pings every link every 20 seconds and
  closes one silent for 70, well inside Cloudflare's 100-second limit for an idle connection. TLS
  ends at the proxy; the hub speaks plain HTTP behind it, on loopback or a private network.
- **Anyone may run a hub.** Worlds trust the hub they link to with their characters' addresses and
  their lines in transit, as they trust their own operators with local tells.

## 5. Persons

A **person** ([01](01-addresses-and-settings.md) section 5) is an account of section 2.2: whoever
signs in with Discord. A person may link characters and list other persons as friends. Nothing of a
person is hidden from another player: a person friend lists every character of its person. The
worlds' half is [02](02-link-protocol.md) section 6 and [03](03-world-behaviour.md) section 9.

### 5.1 What the hub keeps

- **Per person:** section 2.2's Discord user id and name; its linked characters, each an address
  and when it was linked; and its person friends, each a handle, the character it was added from,
  and when. A person has no settings.
- **Per character unlinked:** the handles of the friends handed back to it (section 5.4), its
  world, and when, until its world is sent them or 30 days pass. They name no person.
- **Per character a code was given for:** the code's SHA-256, the character, its world, and when
  the code lapses. The code itself is in the answer to the world, and in memory while the hub
  remembers the request for a resend (5 minutes), nowhere else.
- **Per character of a person that forgot itself:** its address and its world, until its world
  has heard that it is off the network (section 5.3), or the world leaves the registry.
- **Per mirror:** its room, its world (none for the hub's admins'), its Discord channel and
  server, the webhook its operator gave (its id and token) when it goes through one, and when it
  was set, while its world is in the registry (section 5.7); and the hub's own Discord log.
- **Per person and world** it speaks on from Discord: the character it chose to speak as there,
  and when, while that character is its own (section 5.9).
- **Nothing else:** no Discord token, no message, nothing of Discord's but the user's id and name,
  and the mirrors'. The reply windows of tells said in persons' direct messages (which tell went to
  whom, and when), the persons whose direct messages Discord refused within the hour (section 5.8),
  the lines said from Discord that wait for their persons to choose who says them, and the answers
  that wait for their worlds (section 5.9), are held in memory only.
- **A person forgotten** (section 5.3) has each of its characters taken off the network and out
  of the directory, and loses its links, its person friends and every session; each of its
  characters is handed back its friends (section 5.4). An account that is not an operator, one that
  holds no world and has no claim open, goes too, with every report it filed, open or closed, and
  its claims, none of them open; the worlds' logs then name it only as their owners read it
  (section 2.10). What stays is what the limits counted, a day's times alone, under a digest of the
  Discord id. An operator keeps its account and its reports, and loses the events of its links.
- **A world that leaves the registry** (removed, withdrawn or lapsed while pending) takes its
  characters' links, codes and handed-back friends with it, so a world later registered under its
  slug inherits none.
- **The registry's audit log** (section 2.10) records nothing of persons, so a world's operator
  sees no code, link or unlink of its characters. A person's own events (linked, unlinked, a wrong
  code) are kept for the limits, without the characters they were about.
- **A registry an earlier hub kept** loses, when this hub first opens it, what this section no
  longer lists: each person's two settings (whether its characters were shown as one person, and
  whether its tells reached its Discord), the holds persons put on their characters' listings
  (section 6.4), and the events of the first setting's changes.

### 5.2 Link codes

- **A code** is 8 symbols of Crockford's base 32 (`0-9`, `A-Z` without `I`, `L`, `O` and `U`: 40
  random bits), written as two groups of four (`K7QD-2MXP`). An entered code is read without regard
  to case, spaces or hyphens, with `I` and `L` read as `1` and `O` as `0`.
- **It lasts ten minutes and works once.** A character has at most one: a new code replaces it, and
  unlinking spends it.
- **It is the hub's to give**: a world asks for one for its own character (`person {op: "code"}`),
  and the hub answers `unavailable` when it has no sign-in configured. The answer's `url` is a link
  that carries the code: the site's link page (the first configured site origin and `/link/`), `#`
  and the code, `https://dereth.network/link/#K7QD-2MXP`; the page alone when the two together
  would be too long for a result's page. The world shows the player the link.
- **Opening the link links the character.** The page reads the code after `#`, which no server is
  sent, and takes it out of the address bar. A person not signed in is signed in with Discord first
  (section 2.2), and the page comes back to the code. It then links the character
  (`POST /v1/person/link`, section 5.3) with nothing to type, and says which character is linked. A
  link used already, or past its ten minutes, says so, and that `@dn link` in game gives a new one.
  Opened without a code, the page says how to get a link, and takes a code typed there.
- **Limits:** 5 codes an hour for one character; at most 200 codes given to one world's
  characters in any ten minutes (a code's life), spent or not, so that unlinking, which spends a
  code, frees nothing; 10 wrong entries an hour for one person, where a code the hub never gave, a
  spent one and a lapsed one are one wrong entry, answered alike.

### 5.3 The person's routes

Session routes, under section 2.1's rules (the session cookie, the site's origins, JSON bodies):

| route | does |
|---|---|
| `GET /v1/person` | `{"person": ...}`: the person signed in (below), each character with where it stands in the directory. |
| `POST /v1/person/link` `{"code": "K7QD-2MXP"}` | Links the code's character to the person, and spends the code: `200` `{"character": ..., "person": ...}`. A character linked to another person moves to this one, and the person friends that held for it there, and any friends handed back to it and not yet sent, become this person's, added from it, as many as the limit allows. `422` `LINK_FAILED` with `reason` `no_such_code`, alike for a code the hub never gave and a spent or lapsed one; `409` `too_many_characters` past 20 characters; `429` past the limits. |
| `DELETE /v1/person/characters/<address>` | Unlinks one of the person's characters, handing its friends back to it (section 5.4): `204`. `404` for any other address, alike whether another person linked it or nobody did. |
| `DELETE /v1/person/friends/<handle>` | Removes a person friend: `204`; `404` for a handle it has none of. |
| `DELETE /v1/person` | "Forget me": `@dn off` for every character the person links, then the rest. Each character goes out of the directory at once, and its world is told, over its link, to take it off the network (`listing.off`, by its person, [02](02-link-protocol.md) section 7): a linked world within seconds, one away at its next link, however long that is, while it is in the registry. Then every link goes, each character handed back its friends as an unlink does, with its link code; every person friend goes; and every session of the account ends: `204`, and the cookie is cleared. An account that is not an operator (it holds no world and has no claim open) goes with it, and with it every report it filed, open or closed, and its claims, none of them open. A reported world's log still says it was reported, and why. An operator keeps its account and its reports, and signs in again. What the limits (sections 2.9 and 5.2) counted of an account that goes, the times alone, stays for a day under a digest of its Discord id, never the id, and a new sign-in by the same Discord account is held to it. |

```json
{"person": {"name": "Nelly",
            "characters": [{"address": "Ann@frost", "world": {"slug": "frost", "name": "Frost"},
                            "linkedAt": "2026-10-12T09:14:03.000Z", "listing": "listed"}],
            "friends": [{"handle": "Bob@coldeve", "addedFrom": "Ann@frost",
                         "addedAt": "2026-10-12T09:20:41.000Z"}]}}
```

- `world.name` is the registry's, and `null` for a world it does not have.
- `listing` is where the character stands in the directory: `listed`; `taken_down` (held out by
  the hub's operator); or `off`. The person's own page shows all its characters this way, and their
  public pages; no public page shows them together.
- A person's address in a path is the address as written, `@` and all, percent-encoded or not.
- A change made on the site reaches the worlds' person views within a second, or a few seconds
  when it reaches many views (section 5.4).
- A person has no setting to change, and nothing on the site takes a character out of the
  directory: a character is listed while it is on the network, and its player takes it out with
  `@dn off` in game. The routes that did either (`PUT /v1/person/alts`,
  `PUT /v1/person/discord-tells`, `DELETE /v1/person/characters/<address>/listing`) are gone, and
  answer `404` as any route the hub does not have.

### 5.4 Answering worlds

[02](02-link-protocol.md) section 6 gives the requests and the person view. Beside them:

- A person request is held to the per-character pace of a tell and to the world's own (section 3);
  one over either is answered `rate_limited`.
- The hub keeps each world's `person.watch`es while its link lasts, at most 2,000 for one world,
  and sends a view again whenever a link, an unlink or a person friend changes it. A change of a
  person's has the hub read again only the watched views that are its characters' or name one of
  them, since no other view can have changed; a request that changes nothing (removing a person
  friend that is not listed, listing one again, an earlier world's `alts`) has it read none. It
  reads at most 100 views a second, the rest the next second, in the order their persons changed.
- **Every person friend holds for every character of the person**, whichever character it was
  added from. A view names each person friend's handle and every other character of the handle's
  person on the network.
- **A link pauses while its character is off the network.** The hub takes a linked character to be
  on the network while its world lists it in the directory, or the hub's operator holds it out of the
  directory (section 6.4); its world's `listing` that takes it out, at `@dn off`, pauses its link,
  and the one that lists it again, at `@dn on`, resumes it, with nothing to redo, each sending
  again the views that name its person's characters. While it is paused, its person's friends' views
  do not name the character (but as the handle a friend added), no tell to it goes into the person's
  Discord and no reply comes back from it (section 5.8), and the person does not speak as it from
  Discord (section 5.9). Only an unlink breaks the link. A character unplayed for 90 days, whose
  listing lapses (section 6.4), counts as off until its next login lists it again; so does one the
  directory has no room for.
- `befriend` keeps the handle as written, and is refused when the person lists 1,000 person
  friends, from all of its characters together. `carry` lists its targets as `befriend` does, until
  that limit, and is `ok` however many it listed. A person links at most 20 characters.
- **A view holds 100 of them.** A person may list more person friends than a character's friends
  panel holds: the 6096 client's list holds 100 ([03](03-world-behaviour.md) section 2). Past 100,
  the view holds those online now first (a character its world watches the view of), then those
  seen online most lately, then those added most lately, each friend as present as the most present
  character it lists; a friend just added counts as seen then. The hub reads such a view again
  whenever one of the characters its friends list comes online or leaves, or its world's link goes,
  and sends it where that changes which friends it holds. What it remembers of presence is in memory
  alone (section 4).
- **Unlinking hands friends back.** When a character is unlinked (`unlink`, the site's route, or the
  person forgotten), the hub keeps for it the handles of the person friends that held for it then
  (every person friend of its person), at most 100, the first added, and names them in its next
  view, each a handle naming itself alone; it forgets them once that view is sent, or after 30
  days. The person keeps its person friends. A character linked again, by any person, before its
  world was sent them takes them to that person instead.
- The characters a view or `characters` names for a person are its characters on worlds whose
  federation flag is `on`, each while it is on the network itself, in the order they were linked; a
  handle is named always.
- **Worlds of the network's first version** still read the person layer as it was. Every
  `person.friends` carries `"alts": "shown"`, which they need to read the view; it says nothing
  more, since nothing of a person is hidden. Their `characters` request is answered with the
  target's person's characters, as a view names them. Their `alts` request changes nothing: a linked
  character's is answered `refused`, with the first site origin's `/characters/` as its `url`, and a
  character nobody linked's `not_linked`. Newer worlds send neither request.

### 5.5 Privacy

1. **No public route and no operator route** names a person, says whether a character is linked,
   or says which characters are one person; the directory's routes included (section 6.5).
2. **Nothing of a person is hidden from another player.** A person friend lists every character of
   the friend's person on worlds on the network, so a player who lists one of a person's characters
   learns, in game, which characters are that person's. A player keeps a character apart by not
   linking it: a character can be linked only while it is on the network, and one unlinked is, to
   every answer, a character nobody linked.
3. **A world hears of persons through its own characters only**: their views, and the answers to
   their requests. It learns a person's characters as its players do, through the person friends
   its characters list.
4. **An unlinked character** is at once, to every answer, a character nobody linked: its former
   person's views no longer name it, and other persons' views list it as a handle alone. Its own
   view names the friends handed back to it, once.
5. **A world changes a person only through its own character's requests**: a code, an unlink, a
   person friend added or removed. An earlier world's `alts` changes nothing (section 5.4).
6. **The person itself** is shown to nobody but itself, signed in, and `GET /v1/person` shows its
   name, not its Discord id. A world never learns either. The hub's operator, who keeps the
   registry, can read what section 5.1 lists there (the command line's `operators` lists accounts,
   not their characters).
7. **No log** holds a code, and a person's events hold no character's address; the wire log names
   message types only.
8. **Tells into Discord** (section 5.8) say nothing to anyone but the person. The teller is
   answered as for any character not online, so it never learns that the tell went anywhere; the
   direct message names the teller and the one character the tell was for, never the person's
   others; a reply goes back from that character, never another; and the question the hub asks a
   character's world (`reach`) names the teller and that character alone, so the world learns from
   it only that its character is linked, which the character's view already tells it.
9. **Speaking from Discord** (section 5.9) shows the person's characters and its choice among
   them to the person alone, and a line said there shows the one character alone, under its
   address: nobody in the Discord server learns which Discord user spoke as which character, and
   the character's world learns only that its character said a line from Discord, never who the
   person is.

### 5.6 The Discord broker

The Discord application's two secrets, its client secret (section 2.2) and its **bot token**,
belong to the hub alone: no world holds either, runs Discord's code, or is handed a Discord token,
and worlds speak to the hub as they do without the broker. The broker is what the hub does with
the bot token: it mirrors rooms into Discord servers' channels (section 5.7), carries tells into
persons' Discord (section 5.8), and lets persons speak in mirrors as their characters with the
bot's `/dn` command (section 5.9). Without the bot token it is off: `GET /v1/config` says so, no
mirror is kept and no webhook made, Discord's bot API is never called, and sign-in works as before.

The network mirrors rooms into Discord servers' channels, which everyone in a server sees in
Discord's own app, and not into the lobbies of Discord's Social SDK, which only games running the
SDK can see.

**Discord's API.** The hub's calls are the bot's (`Authorization: Bot`, a `User-Agent` of
`DiscordBot (<first site origin>, <version>)`; among them the bot's own direct messages with a
person, section 5.8), or a webhook's, whose token is in its address and on no log, for a line it
says in a mirror. When Discord answers `429`, the hub asks it nothing on that route until the wait
it names is over (as section 2.2 reads a wait), and nothing on any route after one Discord marks
global (`X-RateLimit-Global`, `X-RateLimit-Scope: global`, or its body's `global`); an answer saying
its route has no call left (`X-RateLimit-Remaining: 0`) is waited out the same way, for its
`X-RateLimit-Reset-After`, so the hub is not refused. A call within the wait is answered at once as
waiting, and other routes go on. Discord's address is `https://discord.com/api/v10`, or a stand-in
(`--discord-api`) in development and tests.

**Tokens kept.** The hub keeps no Discord user's OAuth token: sign-in asks for `identify` only and
uses its token once. A store of persons' own Discord tokens, kept
encrypted, waits for the first feature that acts as a person's own Discord account.

### 5.7 Rooms mirrored into Discord

The broker **mirrors** rooms into channels of Discord servers: the Global room, a network channel,
or a world's own room. A mirror is one room in one channel. A room may have many mirrors: the
hub's admins' own, and each world operator's of the Global room, each in its own server. A line
said in the room on any world is said in every one of its mirrors, under its speaker's address
(`Ann@coldeve`). The hub reads no mirror's channel: a Discord user's line there stays in Discord.
A person speaks into the room from a world operator's mirror only as one of its own linked
characters on that world, through that world, with the bot's `/dn say` (section 5.9); the
admins' mirrors are read only. Worlds that know nothing of it speak the link protocol as before.

**Who mirrors what.**

- **The hub's admins** (section 2) mirror the network's rooms: the Global room, and network channels
  by address (`oldguard@channel`). A channel is mirrored while it exists, and one made again under
  its name is mirrored too; its lines reach Discord, so the admins mirror only a channel whose
  members expect that. The admins have one mirror of each.
- **A world's operator** mirrors the world's own rooms (`general`, `trade`, `lfg`, `roleplay`,
  each a room it carries for itself, [02](02-link-protocol.md) section 2, but the one that carries
  the Global room there, section 5.10), and the Global room, into channels of a Discord server the
  operator runs, one mirror of each, with no other bound. The
  world's own rooms' lines go to its own mirrors alone; the Global room's mirror carries every
  world's Global lines, beside the admins' mirror of it. A world's mirrors carry lines while its
  federation flag is `on`, and the registry keeps them while the world is in it: a world registered
  later under the slug inherits none.
- **A channel mirrors one room**, whoever's mirror it is: a channel that mirrors one already is
  refused another (`409`, `channel_taken`).

**Where a mirror goes.** Either way below, the mirror's asker needs to be able to set such a
webhook up itself, so that nobody mirrors a room into another server's channel.

- **Through the bot.** The asker adds the bot to the server at `invite`, the scopes `bot` and
  `applications.commands` (the `/dn` command, section 5.9) with no permissions
  (`permissions=0`), then grants it, on the mirror's channel alone (the channel's settings,
  Permissions, add the bot), View Channel and Manage Webhooks: so the bot sees no other channel
  beyond what any member sees, and reads nothing. The asker names the channel by its id.
  Before the hub keeps it, it asks Discord about the channel as the bot, and refuses it:
  - `409` (`"reason": "bot_cannot_see"`) when the bot cannot see it: the bot is not in its server,
    the channel is hidden from the bot, or there is no such channel;
  - `400` for a channel that is not a server's text or announcement channel;
  - `403` (`not_your_channel`) unless the asker, by the Discord account they signed in with, is a
    member of the channel's server who may view the channel and manage its webhooks;
  - `409` (`bot_lacks_permissions`, its message naming them) unless the bot has View Channel and
    Manage Webhooks in it;
  - `429` while Discord asks the hub to wait, and `503` when Discord cannot be asked.

  Permissions are worked out as Discord works them out: the server's owner and an administrator
  have every one; otherwise the server's everyone role's and the member's roles', then the
  channel's overwrites for everyone, for the member's roles together, and for the member. The bot
  makes a webhook in the channel, named `Dereth Network`, and finds it again at each start and every
  hour after (its own among the channel's webhooks, the oldest should there be more), making another
  only when the channel has none of its own, or at once when Discord says it no longer has it as a
  line is said; the hub keeps that webhook's token in memory alone.
- **Through a webhook the asker made.** In the channel's settings (Integrations, Webhooks), the
  asker makes a webhook and gives the hub its address (`https://discord.com/api/webhooks/<id>/<token>`,
  on `discord.com`, `discordapp.com` or their `ptb.` and `canary.` hosts). Making a webhook needs
  Manage Webhooks in its channel, so the address proves the right, and the bot need not be in the
  server at all. Its channel only receives: without the bot, the server has no `/dn` command. The hub reads the webhook once, with its token, from Discord's own API (never the
  host the address names), learns its server and channel, and refuses an address that is not a
  Discord webhook's (`400`, `not_a_webhook`), one Discord does not have (`409`,
  `unknown_webhook`), and a channel that mirrors another room (`409`, `channel_taken`). The
  registry keeps the webhook's token for the mirror; it is never logged, and no answer, event or
  view holds it or the address. Should Discord no longer have it, the mirror's lines are not
  carried, and the log says its operator needs to give it another.

**The routes.** Each answers `503` on a hub without the broker. A mirror reads:

```json
{"room": "general", "channel": "1558376951000000101", "server": "1558376951000000000",
 "through": "bot", "webhook": "1558376954619629701", "setAt": "2026-10-12T18:20:40.000Z",
 "lines": {"toDiscord": 108, "heldBackToDiscord": 0, "notCarried": 1}}
```

`room` is the room as the routes name it (`global`, `oldguard@channel`, `general`); `through` is
`bot` or `webhook`; `webhook` is the webhook's id (the bot's once made, else `null`), never its
token; `lines` counts what the mirror carried since the hub started (below).

A world's operator, the world's owner alone (`403` for another operator's world, `404` for another's
`pending` one), under section 2.1's session rules:

| route | does |
|---|---|
| `GET /v1/operator/worlds/<slug>/discord/mirrors` | `{"mirrors": [...], "rooms": ["global", "general", "trade", "lfg"], "globalRoom": "roleplay", "invite"}`: the world's mirrors, the rooms it may mirror (never the one carrying the Global room there), where the world carries the Global room (section 5.10; `null` while the hub holds none), and the address the bot is added to a server at (`null` on a hub without sign-in). |
| `PUT /v1/operator/worlds/<slug>/discord/mirrors/<room>` `{"channel": "<channel id>"}` or `{"webhook": "<a webhook's address>"}` | Mirrors the room there, through the bot or through the webhook: `200` and the list, as `GET` answers it. A mirror of the room already is moved there; asking for what is so changes nothing. `400` for a room that is none of the world's nor the Global room, or another body; `409` for a locked world, and for the room that carries the Global room there (`"reason": "carries_global"`: mirror the Global room instead), before Discord is asked anything; and as above. |
| `DELETE /v1/operator/worlds/<slug>/discord/mirrors/<room>` | Stops mirroring it: `204`; `404` for one not mirrored. |

The hub's admins (section 2; `403` for any other operator):

| route | does |
|---|---|
| `GET /v1/admin/discord` | `{"invite", "avatarUrl", "mirrors": [...], "tells": {...}}`: every mirror, the admins' (`"world": null`) and each world operator's (`"world": "<slug>"`), the admins' first and then by world and room; the picture mirrored lines show; and the tells carried into persons' Discord (section 5.8). |
| `PUT /v1/admin/discord/mirrors/<room>` `{"channel"}` or `{"webhook"}` | The admins' own mirror of `global` or of a network channel (`oldguard@channel`, its `@` as written or as `%40`), as a world operator's is set: `200` and the admin view. `400` for another room. |
| `DELETE /v1/admin/discord/mirrors/<room>` | Stops the admins' mirror of the room: `204`; `404` for none. |
| `DELETE /v1/admin/discord/worlds/<slug>/mirrors/<room>` | Takes away a world operator's mirror, of the Global room or any other: `204`; `404` for none. This is how the network moderates the Global room's mirrors in other servers. |
| `PUT /v1/admin/discord/avatar` `{"url": "https://..."}` or `{"url": null}` | The picture every mirrored line shows (none: the webhook's own): `200` `{"avatarUrl"}`. `400` for an address that is not `https://`, longer than 500 bytes, or holding white space or a control character. |
| `GET /v1/admin/discord/log` | `{"events": [{"kind", "at", "actor", "details"}]}`: the hub's own Discord log, newest first, at most 200: the admins' mirrors set and stopped (`discord-mirror`) and the picture set (`discord-avatar`, `{"url"}`), each with the admin who did it. |

Each change to a world's mirror is an event of that world (`discord-mirror`, `{"room", "mirrored":
true, "channel", "server", "through", "webhook"?}`, or `{"room", "mirrored": false}` when it
stops), by its operator, or by the admin who took it away (whom the world's owner reads as
`hub-admin`, section 2.10), or by the hub when the room that carries the Global room changes
(section 5.10: `"was"`, the own room whose mirror carries the Global room now; `"carriesGlobal":
true`, the own room whose mirror went). Each change to the admins' own mirrors, and to the picture,
is an event of the hub's own Discord log. Changes take effect at once.

**A line from the game.**

- A line the routing takes in a mirrored room (a `room.say` it answers `ok`, a member's tell to a
  mirrored channel) is said in each of the room's mirrors, after the worlds are handed it; the
  routing never waits on Discord. A line in a world's own room goes to its own mirror alone.
- Its text passes the line rules of [04](04-text-rules.md), so that Discord shows what every world
  shows, and Discord's mentions (`@everyone`, `@here`, `<@...>`, `<#...>`) are broken by a
  zero-width space after their `@` or `<`, so that no line calls anyone. Every character Discord's
  markdown gives a meaning to (`\`, `*`, `_`, `~`, `` ` ``, `|`, `<`, `>`, `[`, `]`, `(`, `)`, `#`,
  `-`) is escaped by a backslash, except within a link (a word beginning `http://` or `https://`,
  left as written so it still opens), so that the channel shows the characters the worlds show: no
  bold, heading, spoiler or masked link. A line that leaves nothing is not said.
- It is said through the mirror's webhook, showing its speaker's address as its name
  (`Ann@coldeve`) and the picture the admins set, if any. A line its speaker's person said from
  Discord (marked `discord`, section 5.9) shows `(via /dn)` after the address
  (`Ann@coldeve (via /dn)`), since Discord refuses its own name in a webhook's. The request names nobody to call
  (`"allowed_mentions": {"parse": []}`), shows no link's preview (`SUPPRESS_EMBEDS`), and carries no
  credential but the webhook's own. An address holding a word Discord refuses in a webhook's name
  (`discord` or `clyde`, in any case), or one Discord refuses for another reason, is not the line's
  name: the line shows the webhook's name and begins with the address (`Clyde@coldeve: hello`),
  and its mark when it has one (`Clyde@coldeve (Discord): hello`).
- **Discord's limits on a webhook** are its own. Discord names them as it answers (an answer with
  `X-RateLimit-Remaining: 0` says its route has no call left for `X-RateLimit-Reset-After` seconds)
  and by `429`; the hub asks nothing on a route with no call left until it has one, as after a
  `429` (section 5.6). A line waits out a wait of up to 5 seconds, and past that is held back. Each
  mirror's lines are said in order, on one of four threads, so that one server's waits hold up no
  other's.
- **Held back, never silently.** A line is held back, besides, while the bot's webhook is not yet
  made or found, and past 1,000 lines waiting on its mirror's thread; every world is still handed
  it. Every line held back is counted, and the log says when holding begins and why (`holding back
  lines into Discord: Discord asks the hub to wait 40 seconds`), and when lines go again, how many it
  held back. A line the hub may not say at all (its mirror taken away meanwhile, the webhook an
  operator gave gone) is counted as not carried, and logged at most once a minute.

**What it counted.** Each mirror's `lines` says what it carried since the hub started: `toDiscord`,
`heldBackToDiscord` and `notCarried`. Every hour in which the mirrors carried or held back
anything, the log sums the hour up.

**No plain line from Discord.** A Discord user's line in a mirror's channel stays in Discord; no
world is handed one. Only `/dn say` reaches the game, from a person as its own character, through
that character's world (section 5.9). `discord` is a platform word (section 1) and reserved on
the network
([01](01-addresses-and-settings.md) section 1): no world registers it, and a tell, an invitation or
a watch for `Name@discord` is answered as one for a world not on the network (`not_on_network`). A
person reaches Discord through its characters instead: a tell to one of its linked characters goes
to the person's direct messages while that character is not online (section 5.8).

**Privacy.** A line said in a mirrored room reaches Discord, and is Discord's to keep under its own
terms: it stays in the channel, read by everyone in the server and removed by its moderators. A
world says so where it describes the room ([03](03-world-behaviour.md) section 3). The hub keeps no
line: one waits in memory only until it is said or held back. Each character is shown under its
own address, so section 5.5 holds in Discord too: no line or line's name names a person, and none
says which Discord user said a line from Discord.

### 5.8 Tells into Discord

A tell to a linked character, while the character is not online, reaches its person's Discord as a
direct message from the hub's bot, and the person's reply comes back to the teller as a tell from
that character. Worlds speak the link protocol as before and run no Discord code: the character's
world answers one question more (`reach`, [02](02-link-protocol.md) section 8), and the teller's
world takes the reply as any tell. A player on the retail client is told, and answered, as by any
other.

**Who.** There is no setting: linking a character is the person's consent. It holds while the
person links the character, on a world on the network, the character on the network itself (its
link pauses while it is off, section 5.4), its account not disabled; a person forgotten takes
none. Discord lets a bot message a user only while the two share a server and the
user lets that server's members message it, so the person must be in a server the bot is in (the
network's own Discord server, say). When Discord refuses a direct message, the hub tries none in
that person's direct messages for an hour, and reads its replies no more; it keeps that in memory
alone, so a hub that starts again tries at once. A hub carries tells into Discord while it signs
persons in and its broker is on (`broker.discordTells` in `GET /v1/config`).

**Out: a tell into the person's direct messages.**

1. A tell goes to its world as ever, a tell from a character of the same world among them (its
   world sends one to an away, linked character to the hub, [03](03-world-behaviour.md) section 1).
   When the world answers `offline` and the character is linked,
   the teller's world is answered `offline` at once, as for any character not online, and the hub
   asks the character's world `reach`: would the character, not online, have been shown it? The
   question names the teller and the character, nothing more. The world answers by the
   character's own rules: whether it is on the network and is the character that asked for its
   link, its squelches of the teller and of the teller's world, the operator's blocks, and the
   world's pace of tells to it. Anyone else reaches it, a stranger included. A world that answers
   no, or not within 30 seconds, or does not know the question, has the tell go nowhere else.
2. When the world says yes, the bot says the tell in the person's direct messages, unless the
   character told is online now (its world has a `person.watch` for it). Whether the person plays
   another of its characters does not matter. The message reads `Bob@coldeve tells
   Ann@frost, "still playing?"`, with a line under it saying that a reply answers as `Ann@frost`.
   The text passes the line rules of [04](04-text-rules.md) with Discord's markdown escaped, as in a
   mirror (section 5.7); the message calls nobody (`"allowed_mentions": {"parse": []}`) and shows
   no link's preview.
3. Its limits: the teller's paces and the world's, as for any tell ([02](02-link-protocol.md)
   section 5); each person's pace of direct messages, a burst of 10 and then one every 6 seconds;
   at most 1,000 waiting to be said; and the waits Discord names (a `429` or a route with no call
   left, section 5.6), the short ones of up to 5 seconds waited out. A tell held back by any of them
   is counted and logged (at most once a minute), never dropped silently. One Discord refuses, one
   for a person Discord refused within the hour, and one whose person is gone or disabled, is
   counted as not carried.
4. The hub opens the bot's direct messages with the person (`POST /users/@me/channels`), keeps their
   channel's id in memory, and says each tell there (`POST /channels/<id>/messages`), as the bot.

**The teller's answer** is the world's `offline`, `Character not available` in game, exactly as for
a character not online and nobody linked: nothing says that the tell went anywhere, or that the
character is linked (section 5.5). The protocol has no answer that would say it, and none is
added.

**In: the person's reply.**

- **The reply window.** For 24 hours after a tell is said in a person's direct messages, the
  person may answer it; while any tell there is within its window, the bot reads the direct
  messages, after the last line it read (the first read after a tell opens the window takes
  nothing said before it), every `--discord-poll-secs` (3), all persons' together at most 5 reads
  a second, so that past 15 open windows each is read less often. Past every window, nothing is
  read. Discord gives a bot the text of its own direct messages, so the application needs no
  privileged intent for them. The windows are held in memory: a hub that starts again reads no
  reply to a tell said before, until a new tell opens a window.
- **What answers what.** A line the person wrote there answers the tell it replies to (Discord's
  Reply), while that one is within its window; otherwise the latest tell within it. Only the
  person's own lines and replies are read: never a bot's (the hub's tells and notes), a webhook's or
  Discord's own, nor anything but a line or a reply.
- **Back as a tell.** The reply goes to the teller's world as a tell from the character the tell was
  for (`Ann@frost`), to the teller (`Bob@coldeve`), held to the paces a tell from that character's
  world is and the loop guard ([02](02-link-protocol.md) section 5), `friend` when a person friend
  of the character's person lists the teller (worlds of the network's first version read it). Its
  text is made plain (a mention of a user shows as `@` and their name, a role's as `@role`, a
  channel's as `#channel`, a custom emoji as `:name:`), led by a note of its attachments
  (`[attachment]`, `[3 attachments]`), and passes the line rules; one that leaves nothing is not
  carried. The teller's world takes it as any tell: whether the teller is on the network, its
  squelches and its world's blocks decide. The person answers as that character, never as another. A
  reply is not carried once the character is no longer the person's, is off the network itself, its
  world or the teller's is off the network, or the person's account is gone or disabled.
- **A teller who is away.** When the teller's world answers the reply `offline` and the teller's
  character is linked, the reply goes on as any tell to a character not online does (Out, above):
  the teller's world is asked `reach`, whether the replying character may reach the teller, and when
  it says yes the reply is said in the teller's person's direct messages. So two persons who are
  both away answer each other there, each line held to the paces of its character's tells and to the
  loop guard, as any two characters' tells are, and nothing answers by itself: every line is a
  person's own.
- **Its answer** is told the person as the game would tell the character: a line of the bot's
  saying that the teller is away (with its away text), is not available, or that the network is
  holding its tells back for now; nothing for a reply shown, or one the teller squelches. A reply
  that went on to the teller's direct messages is answered not available all the same, as any
  teller's tell is: nothing says that it went anywhere, or that the teller is linked.

**What it counted.** `GET /v1/admin/discord` (section 5.7) answers `tells`: `{"toDiscord",
"fromDiscord", "heldBackToDiscord", "heldBackFromDiscord", "notCarried", "replyWindows"}`, what
went each way since the hub started, and how many persons' direct messages are read now.

**Privacy.** A tell said in a person's direct messages is Discord's to keep, under its own terms,
as the person's own messages are; the hub keeps neither, and logs neither. Of a reply window it
keeps which tells went to the person, from whom and for which character, in memory, for the
window. Section 5.5's eighth rule holds throughout.

**Discord's Social SDK** (a player's rich presence, Ask to Join, direct messages from inside the
game) is the game client's and the launcher's, on the player's own machine, which know what the
player plays and where; it is not the hub's. None of it may change section 5.5: the hub names a
person's Discord account to nobody but the person.

### 5.9 Speaking from Discord

A person speaks in a world operator's mirror of a room (section 5.7) as one of its own linked
characters on that world, through that world, with the bot's `/dn` command. Mirrors are for the
network's players to talk out of game, not for any Discord user to reach the game: only `/dn say`
reaches it, only as a character the person linked (section 5.2) and on the network, its one such
character on that world or the one it chose, and only where the world says the line under its own
rules. Plain lines in a
mirror's channel stay in Discord, and the hub reads none. A hub offers it while it signs persons in
and its broker is on (`broker.speak` in `GET /v1/config`); otherwise the route answers `503`.

**How Discord reaches the hub.** Discord sends each use of the command to the application's
Interactions Endpoint URL, `POST /v1/discord/interactions` on the hub's public address, which the
hub's operator sets in Discord's developer portal (`network/hub/DEPLOY.md`). The hub opens no
connection to Discord's Gateway, and needs no privileged intent.

- **Signed.** The hub takes a request only when its `X-Signature-Ed25519` is the Ed25519 signature,
  under the application's public key, of its `X-Signature-Timestamp` followed by its body, and the
  timestamp is within 5 minutes of the hub's clock; every other is answered `401`, logged at most
  once a minute. The hub reads the public key from Discord at its start, with the bot token
  (`GET /applications/@me`, its `verify_key`), and asks again after a failure; until it has it, it
  answers every request `401`. An interaction the hub took within the last 5 minutes, sent again,
  is answered `401` too. A signed interaction takes nothing from its client address's allowance
  (section 1), since every one comes from Discord's own addresses; a request that fails the check
  takes from it, and is answered `429` once it is spent.
- **Answered at once.** Discord waits 3 seconds for an answer. `PING` is answered `{"type": 1}`;
  every other answer is a message the person alone sees (ephemeral), naming nobody to call. A line
  for a world is answered by deferring, also ephemerally, and the hub edits that answer once the
  world has answered (`PATCH /webhooks/<application>/<the interaction's token>/messages/@original`,
  the token in memory alone, never logged, and forgotten after 15 minutes).
- **The command.** At its start the hub registers `/dn` with the bot token
  (`PUT /applications/<application>/commands`, which replaces the application's commands with the
  same one, so registering again changes nothing), and asks again after a failure. `/dn` is offered
  in servers alone, with two subcommands: `/dn identity`, and `/dn say <text>`, its text at most 255
  characters.

**`/dn identity`** chooses who the person speaks as on the channel's world. Its answer names the
person's characters on that world (those it links and that are on the network, section 5.4) and the
one it speaks as now, with a menu of them; a choice made there is kept at once, and the answer says
so. A choice of a character that goes off the network is kept, and holds no more until the
character is back on. The choice
is kept per person per world, not per server: a server may hold several worlds' mirrors, each
channel belonging to one world. It is only ever one of the person's own characters on that world,
and it goes when the character stops being the person's: unlinked (on the site, by the world's
`unlink`, or linked by another person), said by its world to be no character of its own or not the
one that asked for its link (a character deleted, or renamed), the person forgotten, or the world
leaving the registry. Choosing again replaces it.

**`/dn say <text>`** says a line in the channel's room as the person's character on the channel's
world: the one it chose, or else its one character there, of those on the network. A person with several characters there
and none chosen is answered with `/dn identity`'s menu, which says that the line is said as the one
chosen. The line waits for the choice, in memory alone, as long as Discord lets the hub edit its
answer (15 minutes); at most 1,000 lines wait for all persons together, the oldest going first. A
choice in that menu is kept, as one in `/dn identity`'s is, and says the line; a choice made after
the line has gone says that it was not said.

1. The hub finds the mirror in the channel, and the person by the Discord account that used the
   command (a person whose account is disabled is none). Its text is made plain as a reply's is
   (section 5.8) and passes the line rules of [04](04-text-rules.md); one that leaves nothing is
   not said.
2. It is held to the person's pace, a burst of 3 and then one every 5 seconds, beside the world's
   own pace of its character's room lines.
3. The hub asks the character's world to say it (`say`, [02](02-link-protocol.md) section 9): in
   the mirror's room (`global`, or the world's own), as the character, marked `via` `discord`. A
   world whose link is down, or that does not carry the room, is not asked.
4. The world says it under its own rules ([03](03-world-behaviour.md) section 3), shows it to its
   own players, and sends it on as any line of the room; the hub hands it to the other worlds
   carrying the room and to every mirror of the room, this channel among them, its name marked
   (section 5.7). The world's answer is the person's.

**The answers**, each the person's alone:

| when | the person is told |
|---|---|
| the channel mirrors no room | *This channel mirrors no room of the Dereth Network.* |
| an admins' mirror | *This channel mirrors the network's Global room for everyone to read. Speak in it from your own world's Discord, or in game.*; of a channel, *This channel mirrors the channel oldguard@channel for everyone to read. Speak in it in game.* |
| a choice that is not one of the person's characters there | *That is not one of your characters on Coldeve.* |
| the person links no character on the world, or is no person | *You have no character linked on Coldeve. To link one, type @dn link in game and open the link it gives you.* |
| the person's characters linked on the world are all off the network | *Your characters linked on Coldeve are off the Dereth Network. Type @dn on in game to speak as one.* |
| `identity` | *Choose which of your characters on Coldeve you speak as.*, then, while it speaks as one, *You speak as Ann@coldeve now.*; with the menu |
| `say`, several characters on the world and none chosen | *Choose which of your characters on Coldeve you speak as. Your line is said as the one you choose.*, with the menu |
| a choice in either menu | *You speak as Ann@coldeve on Coldeve now.*; in `say`'s menu, followed by *Saying your line...*, and then by how the line ended (below), or by *Your line waited too long and was not said. Say it again with /dn say.* |
| `say`, nothing left of the text | *There is nothing to say.* |
| `say`, over the person's pace | *You are speaking too quickly. Wait a moment and try again.* |
| `say`, the world off the network or its link down | *Coldeve can't be reached right now.* |
| `say`, the world not carrying the room | *Coldeve does not carry this room now.* |
| `say`, the world refused | *Ann@coldeve did not say it:* and the reason's words, below |
| `say`, no answer within 10 seconds | *Coldeve did not answer in time.* |
| `say`, said | *Said as Ann@coldeve.* |

The reasons' words: `no_character`, *Coldeve has no such character now. Choose another with /dn
identity.*; `not_on_network`, *it is not on the Dereth Network. Type @dn on in game.*;
`not_linked`, *Coldeve says it is not the character you linked. Link it again with @dn link in
game.*; `banned`, *its account is banned on Coldeve.*; `gagged`, *it is gagged on Coldeve.*;
`too_new`, *it is too new to speak on the Dereth Network yet.*; `rate_limited`, *it is speaking too
quickly. Wait a moment and try again.*; `room_off`, *Coldeve does not carry this room now.*;
`not_allowed`, *it may not speak in this room on Coldeve.*; any other, *Coldeve refused it.* The
first and the third forget the choice. Every name in an answer is written as a mirror writes a line
(section 5.7), Discord's markdown escaped.

**Privacy.** Section 5.5's ninth rule. The person's characters, its choice and the answers go to
the person alone, as ephemeral messages, which leave nothing the server sees; the line is said by
the mirror's webhook under the character's address, never by the person's Discord account. The
character's world is told the character, the room and the text, never the person or its Discord
account. The hub logs no line, no interaction's token, and no Discord user beside a character.

### 5.10 A world's rooms

One implementation: `network/hub/src/rooms.rs`, with the routing's side in `core.rs`.

The hub keeps, for each world, which of the world's own rooms (`general`, `trade`, `lfg`,
`roleplay`) carries the Global room, or none (`off`), and tells the world, with the own rooms it
mirrors into Discord for it (section 5.7), at its link's `welcome` and whenever either changes
([02](02-link-protocol.md) section 10). The world carries them as it is told
([03](03-world-behaviour.md) section 3). Before a world's room is known (a world that never said,
or that predates it), the hub holds none.

**Who sets it.**

- **The world's operator**, the world's owner alone (`403` for another operator's world, `404` for
  another's `pending` one), under section 2.1's session rules, on a hub with or without the broker:

  | route | does |
  |---|---|
  | `GET /v1/operator/worlds/<slug>/global-room` | `{"globalRoom": "roleplay"}`: where the world carries the Global room; `null` while the hub holds none. |
  | `PUT /v1/operator/worlds/<slug>/global-room` `{"room": "trade"}` | Carries it there (`general`, `trade`, `lfg`, `roleplay`), or nowhere (`off`), at once: `200` and the room, as `GET` answers it. Asking for what is so changes nothing. `400` for another room or body; `409` for a locked world. |

  The mirrors' `GET` answers it too, as `globalRoom` (section 5.7).
- **The world itself**, at its operator's word in game (`/dn admin global`,
  [03](03-world-behaviour.md) section 6), with `rooms.global` on its link, under its key: no site
  and no Discord needed. It is held to its pace ([02](02-link-protocol.md) section 5).
- **The world's settings, once.** A world told no room sends where it carries the Global room (its
  seed); the hub keeps a seed only while it holds no room for the world, so once it holds one, the
  world's settings change nothing.

Each change is an event of the world's (`global-room`, `{"room", "was", "seed"?}`, section 2.10), by
the operator who made it, or by `world`; one that changes nothing is none. A world removed from the
registry takes its room with it: one registered later under the slug inherits none.

**Mirrors follow it.**

- **The room that carries the Global room is not one of the world's own** while it does: a mirror
  of it is refused (`409`, `carries_global`, section 5.7), and the world is never told it is
  mirrored.
- **When the room becomes known or changes**, the world's mirror of the room that now carries the
  Global room, if it has one, becomes the world's mirror of the Global room, in the same channel,
  when the world has none; otherwise it is taken away, since the Global room is mirrored already.
  Either way the log says so, and an event of the world's records it, by `hub` (section 5.7). So a
  world whose operator mirrored the room that carries the Global room there, before the hub knew,
  finds the Global room's lines in that channel.
- **A room that stops carrying the Global room** is an ordinary own room again, mirrored nowhere
  until its operator mirrors it.

## 6. The character directory

One implementation: `network/hub/src/directory.rs`, with the routing's side in `core.rs`.

The directory is the hub's public list of the characters on the network
([01](01-addresses-and-settings.md) section 2): each one's current profile, as its world last sent
it ([02](02-link-protocol.md) section 7), which anyone may search on the site. It shows each
character and its allegiance's facts, its listed patron and monarch linked; it draws no allegiance
trees.

### 6.1 What the hub keeps

- **Per listed character:** its address, its world, the profile last sent, the hour that profile
  last changed (never the minute), and the day a world last sent it.
- **Per character held out:** its address, its world, whether its world has yet said it heard,
  who held it out, and until when it is held: the hub's operator for 30 days; a forgotten person's
  character until its world has heard (section 6.4).
- **Nothing else:** no earlier profile, no history of levels or titles, no time finer than an
  hour, no log that names a character (the wire log names message types only). Taking a character
  out deletes its row: the directory keeps no hidden copy.
- **Bounds:** at most 20,000 listings of one world's characters (a new one past that is refused
  `full`). A full profile, with 50 skills and 40 augmentations, is about 3.7 KiB, and a listing
  takes about 4 KiB of the registry with its indexes, so 5,000 listed characters take about 20
  MiB.

### 6.2 Taking listings

- A world lists only its own characters (`identity` otherwise), and only while it is on the
  network: a listing comes over a federated world's link.
- A listing that is `on` is refused `taken_down` while the character is held out, until the hub's
  operator's 30 days are up, whatever its world does. A listing that is not `on` is the world
  saying it heard, and the hold stays its time.
- A profile sent again unchanged changes nothing published, not even the hour. A changed one is
  published at once, with the hour it changed.
- A listing that is not `on` deletes the character's row at once, and its address from every
  listing that names it as a patron or a monarch.
- **Pace.** [02](02-link-protocol.md) section 5's: per character, per world, and 100 a second for
  every world together. A changed profile is one write to the registry, about a millisecond on a
  solid-state disk; one sent unchanged is a read, and at most one write a day. So at the hub's
  bound the routing spends at most a tenth of its time on listings, and a world reconnecting, or
  several, with hundreds of listed characters online holds up no tell. A network of a few
  thousand listed characters, each sending at most once in 5 minutes and at its logout, sends a
  few listings a second at its busiest.
- The answer's `url` is the character's page on the first site origin:
  `/directory/character/?a=` and the address, percent-encoded, when that is at most 200
  characters.

### 6.3 The public routes

They answer `Access-Control-Allow-Origin: *`, never read the cookie, and read nothing of the
person layer.

| route | answers |
|---|---|
| `GET /v1/directory/characters?q=&world=&level_min=&level_max=&heritage=&allegiance=&sort=&limit=&offset=` | a page of listed characters: `{"data", "total", "limit", "offset", "nextOffset"}` |
| `GET /v1/directory/characters/<address>` | one listed character: `{"character": ...}`; `404` for any other address |
| `GET /v1/directory/allegiances/<world>/<key>?limit=&offset=` | an allegiance's listed members, highest rank first, then by name: `{"allegiance", "data", "total", "limit", "offset", "nextOffset"}`; `404` when none is listed |

**A search's fields**, each refused `400`, naming it, out of its bounds:

- `q`: the start of the name, or of any word of it, without regard to case: 2 to 32 letters,
  spaces, apostrophes and hyphens, at least two of them letters;
- `world`: a world's slug; `heritage`: 1 to 13; `level_min` and `level_max`: 1 to 100,000, the
  first no more than the second;
- `allegiance`: the start of the allegiance's name, or of any word of it, without regard to case:
  2 to 40 characters of shown text, matched as written (none is a wildcard);
- `sort`: `updated`, the default, the profiles that changed most lately first; or `level`, the
  highest first; then by name, either way, over every world;
- `limit`: 1 to 50 (50 when left out); `offset`, from 0. A search reaches its first 1,000 results:
  `offset` and `limit` together at most 1,000, past which the answer is `400`, to narrow it.

Each result names `address`, `name`, `world` (`slug` and `name`), `updated` (the hour the profile
last changed), `level`, `heritage`, `gender`, and `title` and `allegiance` (`key`, `name`, `rank`)
where the profile has them. A character's own answer has the same, then every other field of its
profile, but its allegiance's `patron` and `monarch` only while that character is listed itself:

```json
{"character": {"address": "Sir Bob@coldeve", "name": "Sir Bob",
  "world": {"slug": "coldeve", "name": "Coldeve"}, "updated": "2026-10-12T14:00:00.000Z",
  "level": 126, "heritage": 1, "gender": 1, "title": "the Bloodletter",
  "allegiance": {"key": "k3Yq0pW9xLmN", "name": "Order of the Dawn", "members": 41, "rank": 3,
                 "patron": "Cara@coldeve"},
  "skills": [{"id": 6, "training": "specialized", "base": 420}], "deaths": 12}}
```

An allegiance's answer has `allegiance` (`world`, `key`, `name` when it has one, `members`, as
its most lately changed listed member gives them, and `listed`, how many are listed), and each
listed member as a search result with its `patron`, while listed.

- **Absence is one answer.** A character nobody listed, one taken out, one held out, one of a
  world off the network, and an address that names no character all answer `404` with the same
  body; an allegiance with no listed member and a key that names none, likewise. No answer says
  that a name exists.
- **Caching.** A `200` carries `Cache-Control: public, max-age=60`, so a cache in front of the hub
  may answer it for a minute, and a character taken out may linger there that long. A `404`
  carries `no-store`.
- **Searches** are held to 30 a minute from one client address, beside every request's
  allowance (section 1), and answered `429` with `Retry-After` past it. A character's page and an
  allegiance's members are held only to the allowance.
- No route lists every character at once, and nothing exports the directory.

### 6.4 Taking characters out

| how | what happens |
|---|---|
| the player: `@dn off`, the character deleted or renamed | its world's listing deletes the row at once |
| the hub's operator, `dereth-network-hub directory remove <Name@world>` | the row is deleted at once (none, for a character not listed); the character is held out for 30 days, whatever its world does, and its world is sent `listing.off` (`operator`); the world's log records `listing-taken-down` and the address, for its operator |
| its person forgetting itself on the site (section 5.3) | the row is deleted at once; its world is sent `listing.off` (`person`), however long it is away, and takes the character off the network; the hub holds the character out until its world has heard, and no longer, unless the operator's hold on it runs longer |
| its world leaving the network (its flag leaving `on`: turned off, revoked, locked or unlisted, or its operator disabled) or the registry (removed, or lapsed while pending) | every listing and every hold of the world is deleted at once, but the offs it owes hearing of forgotten persons' characters, which it is sent when it comes back, while it is in the registry; a world that comes back lists its characters again as they log in |
| no world sending the character's listing for 90 days | the listing lapses. A world lists a listed character at each login, so only one unplayed for 90 days, or gone, lapses; it is listed again at its next login |

- Whenever a listing goes, its address goes too from every listing that named it as a patron or a
  monarch.
- A world is sent each `listing.off` it owes at its link's `welcome`, and within 5 seconds of one
  made later (the command line acts on the registry of a running hub), once on each link until
  the world says the character is off.
- A world merely offline, its link down, keeps its listings.
- Nothing on the site takes one character out: a character is listed while it is on the network.
  Forgetting the person takes all of its characters off the network.

### 6.5 Privacy

- **The directory never reads the person layer** for a public answer: no field, filter, sort or
  count depends on whether a character is linked, or on its person. A person sees its own
  characters' standing on its own routes alone (section 5.3), and nothing public groups
  characters.
- **What a world sends is what is published,** field for field: the hub keeps no field a profile
  does not name, and the world leaves out what its character's options hide
  ([02](02-link-protocol.md) section 7).
- **No presence.** The directory never says whether a character is online. It shows the hour a
  profile last changed, and keeps no finer time.
- **Unlisted characters are never named.** A world names a character's patron and monarch only
  while each is listed, and the hub publishes them only while each is: the name of an unlisted
  member never reaches the hub. An allegiance's size counts them, and nothing else does.

**Known limits.**

- **Activity.** A listed profile changes while its character plays, and the hub publishes each
  change at once: whoever polls the directory sees when each listed character's profile changes,
  and so roughly when it plays, whatever hour a page shows. Comparing two listed characters'
  changes over weeks may suggest that they are one player's, since a player plays one character at
  a time. What `@dn on` says in game tells the player that the profile changes as it plays
  ([03](03-world-behaviour.md) section 10).
- **Dates of birth.** A date of birth that a character's options show is its creation time: two
  characters made minutes apart, both showing theirs, may be taken for one player's.
- **Names and titles.** Two listed characters with the same unusual name or title link
  themselves: the directory groups nothing, but a reader can.
- **What a world says.** The hub can check nothing a world says of its own characters. A world
  speaks only for its own; every page shows its world; its listings are bounded and capped; and a
  report to the hub's operator, and a revoke, take every listing of a world that lies out.
