# The hub

Provenance: **network specification**.

One implementation: `network/hub/` (`dereth-network-hub`): `registry.rs` (the registry),
`operators.rs` (world operators, their sessions, verification, approval and reports),
`community.rs` (the community list), `persons.rs` (persons: links, codes, the alts setting and
person friends), `core.rs` (the routing, with no I/O), `server.rs` (the listener), `api.rs` (the
operator API and the person routes), `challenge.rs` (the challenges verification in game sends),
`fetch.rs` (the hub's own outbound requests), `forwarded.rs` (client addresses behind a
proxy) and `self_update.rs` (installing the hub's own releases). The challenge datagram itself is `dereth_network_protocol::challenge`. `network/hub/DEPLOY.md` is the deployment guide.

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
  A world still `pending` 14 days after it was registered (`--pending-days`) is removed, and its
  slug is free again; its operator's log says so.
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
| `/v1/person...` | a person's own routes: linking characters, the alts setting, person friends (section 5) |

The lists of every world (`/v1/servers`, `/v1/status`, `/v1/network`, and the two compatibility
feeds) are answered from a snapshot of the registry at most a second old, and never wait on the
registry while something else holds it (the last snapshot is answered then, however old); they
carry `Cache-Control: public, max-age=10`, so a cache in front of the hub can take a burst. Every
other answer carries `Cache-Control: no-store`.

Each world in the public answers carries `links`: `web`, the site's list of worlds searched for
it (`<site>/worlds/?q=<slug>`, on the first `--site-origin`), and `status` and `resolve`, the
hub's own `<public URL>/v1/servers/<slug>/status` and `<public URL>/v1/resolve/<slug>`. A hub given
neither takes itself to be at `https://hub.<domain>` and its site at `https://<domain>`. A world
has no host name of its own under the domain: where players connect is its `endpoint`.

`GET /v1/config` answers `{"baseDomain", "heartbeatTtlSeconds", "authEnabled", "signIn"}`, where
`authEnabled` says whether operators can sign in on this hub and `signIn` is the route that starts
it (`/v1/auth/discord`). A `pending` world, a locked one (section 2.8) and an unlisted one
(section 2.12) are in none of the public routes: not the lists, not `Servers.xml` or the population
counts, not `/v1/servers/<slug>` or `/v1/resolve`.

Errors are `{"error":{"code","message"}}` with the status (`400`, `401`, `403`, `404`, `408`,
`409`, `411`, `413`, `422`, `429`, `503`). Every `429`, whatever limit it is, carries `Retry-After` and
`"retryAfterSeconds"` in its error, both the whole seconds until a retry can succeed; every answer
with CORS headers also carries `Access-Control-Expose-Headers: Retry-After`, so a page on another
origin can read it. Requests, and link attempts before their key is read, are limited per client
address (180 a minute by default; an IPv6 client counted by its /64, as its connections are),
counted once the request's head has arrived and before its body is read, so a refused request is
answered at once (the hub then takes in what the client is still sending, up to 64 KiB for at most
a second, before it closes, so that the answer is not lost to a reset); a refused link is logged at most once a minute. At most 2,048
connections are open at once, and at most 32 from one address that is not a trusted proxy (an IPv6
client counted by its /64, since one host is commonly given a whole /64). A request's head may take
10 seconds to arrive, or 2 while half the connections are still sending theirs; its body may take
10 seconds in all, however it trickles in, or the request is answered `408` (`TIMEOUT`) and closed.

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
`sso`, `id`, `identity`, `console`.

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
them answers every operator route `503` and works otherwise as before.

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

- **The operator's Discord account** must be at least 30 days old (`--min-account-age-days`), by
  the time its id says it was made, to register a world or claim a listing (`403`, `reason`
  `account_too_new`), so that accounts made by the dozen cannot.
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
                  "game": "/dn verify dnv_q0Zx4l..."}}
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

**A command in game.** An administrator of the world types `/dn verify <token>` (`@dn verify` works
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
accounts cannot spend it for everyone). The answer's `approval` says which:

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

Every request takes from its client address's allowance (section 1). The operator routes are also
held to these, each answered `429` with `Retry-After` past it:

| what | limit |
|---|---|
| registrations | 3 a day for one operator, and at most 10 worlds and claims held at once |
| open claims | 3 for one operator, and 5 claims opened a day, withdrawn or not |
| new verification tokens | 10 a day for one operator's verification of one world |
| verification attempts (page or game) | 10 an hour for one operator's verification of one world, counted once the key is the claimant's |
| page fetches | 30 an hour for one operator, and a ceiling of 3,000 an hour for the hub; a page the hub fetches, never a URL refused before (not on the world's host) |
| federation approvals | 3 a day for one operator; the hub's daily ceiling for all (500 by default, `--approvals-per-day`) |
| new keys | 10 a day for one world |
| registration changes | 30 a day for one world |
| reports | 5 a day for one operator |

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
hub's operator), key issued, rotated or revoked, reported, report resolved, locked and unlocked.
Sign-in and sign-out are the operator's own events. No key, token, session id, Discord token,
client secret or fetched page is ever written to the log or the database.

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
  seconds, while its target world reconnects. The log names message types and worlds.
- **No presence history**, no telemetry beyond the registry's heartbeat.
- **No game accounts.** The hub knows a character only as an address its home world vouches for.
  The only accounts are Discord users who signed in (section 2): a Discord id, a name and sign-in
  times, and, for a person who links characters, section 5's links, setting and person friends,
  which nobody else learns while the person hides its alts.
- **Behind a proxy.** A proxy in front of the hub must pass WebSocket upgrades on, and its idle
  timeout must be longer than the link's keep-alive: the hub pings every link every 20 seconds and
  closes one silent for 70, well inside Cloudflare's 100-second limit for an idle connection. TLS
  ends at the proxy; the hub speaks plain HTTP behind it, on loopback or a private network.
- **Anyone may run a hub.** Worlds trust the hub they link to with their characters' addresses and
  their lines in transit, as they trust their own operators with local tells.

## 5. Persons

A **person** ([01](01-addresses-and-settings.md) section 5) is an account of section 2.2: whoever
signs in with Discord. A person may link characters, set whether its characters are shown as one
person, and list other persons as friends. The worlds' half is [02](02-link-protocol.md) section 6
and [03](03-world-behaviour.md) section 9.

### 5.1 What the hub keeps

- **Per person:** section 2.2's Discord user id and name; the alts setting (`hidden` until the
  person changes it); its linked characters, each an address and when it was linked; and its
  person friends, each a handle, the character it was added from, and when.
- **Per character unlinked:** the handles of the friends handed back to it (section 5.4), its
  world, and when, until its world is sent them or 30 days pass. They name no person.
- **Per character a code was given for:** the code's SHA-256, the character, its world, and when
  the code lapses. The code itself is in the answer to the world, and in memory while the hub
  remembers the request for a resend (5 minutes), nowhere else.
- **Nothing else:** no Discord token, no message, nothing of Discord's but the user's id and name.
- **A person forgotten** (section 5.3) loses its setting, its links and its person friends. An
  account that is not an operator, one that holds no world and has no claim open, goes too, with its
  sessions, every report it filed, open or closed, and its claims, none of them open; the worlds'
  logs then name it only as their owners read it (section 2.10). What stays is what the limits
  counted, a day's times alone, under a digest of the Discord id. An operator keeps its account and
  its reports.
- **A world that leaves the registry** (removed, withdrawn or lapsed while pending) takes its
  characters' links, codes and handed-back friends with it, so a world later registered under its
  slug inherits none.
- **The registry's audit log** (section 2.10) records nothing of persons, so a world's operator
  sees no code, link or unlink of its characters. A person's own events (linked, unlinked, the
  setting changed, a wrong code) are kept for the limits, without the characters they were about.

### 5.2 Link codes

- **A code** is 8 symbols of Crockford's base 32 (`0-9`, `A-Z` without `I`, `L`, `O` and `U`: 40
  random bits), written as two groups of four (`K7QD-2MXP`). An entered code is read without regard
  to case, spaces or hyphens, with `I` and `L` read as `1` and `O` as `0`.
- **It lasts ten minutes and works once.** A character has at most one: a new code replaces it, and
  unlinking spends it.
- **It is the hub's to give**: a world asks for one for its own character (`person {op: "code"}`),
  and the hub answers `unavailable` when it has no sign-in configured. The answer's `url` is the
  site's page for entering it: the first configured site origin and `/characters/`.
- **Limits:** 5 codes an hour for one character; at most 200 codes given to one world's
  characters in any ten minutes (a code's life), spent or not, so that unlinking, which spends a
  code, frees nothing; 10 wrong entries an hour for one person, where a code the hub never gave, a
  spent one and a lapsed one are one wrong entry, answered alike.

### 5.3 The person's routes

Session routes, under section 2.1's rules (the session cookie, the site's origins, JSON bodies):

| route | does |
|---|---|
| `GET /v1/person` | `{"person": ...}`: the person signed in (below). |
| `POST /v1/person/link` `{"code": "K7QD-2MXP"}` | Links the code's character to the person, and spends the code: `200` `{"character": ..., "person": ...}`. A character linked to another person moves to this one, and the person friends that held for it there, and any friends handed back to it and not yet sent, become this person's, added from it, as many as the limit allows. `422` `LINK_FAILED` with `reason` `no_such_code`, alike for a code the hub never gave and a spent or lapsed one; `409` `too_many_characters` past 20 characters; `429` past the limits. |
| `DELETE /v1/person/characters/<address>` | Unlinks one of the person's characters, handing its friends back to it (section 5.4): `204`. `404` for any other address, alike whether another person linked it or nobody did. |
| `PUT /v1/person/alts` `{"shown": true}` | The alts setting: `{"person": ...}`. |
| `DELETE /v1/person/friends/<handle>` | Removes a person friend: `204`; `404` for a handle it has none of. |
| `DELETE /v1/person` | Forgets the person: every link, every person friend, and the setting; each of its characters is handed back its friends, as an unlink does. An account that is not an operator (it holds no world and has no claim open) goes with it, and with it its sessions, every report it filed, open or closed, and its claims, none of them open: `204`, and the cookie is cleared. A reported world's log still says it was reported, and why. An operator keeps its account and its reports: `204`, signed in still. What the limits (sections 2.9 and 5.2) counted of an account that goes, the times alone, stays for a day under a digest of its Discord id, never the id, and a new sign-in by the same Discord account is held to it. |

```json
{"person": {"name": "Nelly", "alts": "hidden",
            "characters": [{"address": "Ann@frost", "world": {"slug": "frost", "name": "Frost"},
                            "linkedAt": "2026-10-12T09:14:03.000Z"}],
            "friends": [{"handle": "Bob@coldeve", "addedFrom": "Ann@frost",
                         "addedAt": "2026-10-12T09:20:41.000Z"}]}}
```

- `world.name` is the registry's, and `null` for a world it does not have.
- A person's address in a path is the address as written, `@` and all, percent-encoded or not.
- A change made on the site reaches the worlds' person views within a second, or a few seconds
  when it reaches many views (section 5.4).
- Changing the alts setting, from the site or by hiding them in game, is held to 20 times a day
  for one person. Only the site shows them: a world's `alts` with `shown` is refused, with the
  first site origin's `/characters/` as its `url`.

### 5.4 Answering worlds

[02](02-link-protocol.md) section 6 gives the requests and the person view. Beside them:

- A person request is held to the per-character pace of a tell and to the world's own (section 3);
  one over either is answered `rate_limited`.
- The hub keeps each world's `person.watch`es while its link lasts, at most 2,000 for one world,
  and sends a view again whenever a link, an unlink, a setting or a person friend changes it. A
  change of a person's has the hub read again only the watched views that are its characters' or
  name one of them, since no other view can have changed; a request that changes nothing (hiding
  characters already hidden, removing a person friend that does not hold, listing one again) has
  it read none. It reads at most 100 views a second, the rest the next second, in the order their
  persons changed.
- `befriend` keeps the handle as written, and is refused when 100 person friends hold for the
  asking character: those added from it while its person hides its alts, all of the person's while
  it shows them. So a hidden person's other characters change no answer. `carry` lists its targets
  as `befriend` does, until that limit, and is `ok` however many it listed. A person links at most
  20 characters.
- **Unlinking hands friends back.** When a character is unlinked (`unlink`, the site's route, or
  the person forgotten), the hub keeps for it the handles of the person friends that held for it
  then, at most 100, and names them in its next view, each a handle naming itself alone; it forgets
  them once that view is sent, or after 30 days. The person keeps its person friends. A character
  linked again, by any person, before its world was sent them takes them to that person instead.
- The characters a view or `characters` names for a person are its characters on worlds whose
  federation flag is `on`, in the order they were linked; a handle is named always.

### 5.5 Privacy

1. **No public route and no operator route** names a person, says whether a character is linked,
   or says which characters are one person.
2. **A world hears of persons through its own characters only**: their views, and the answers to
   their requests. While a person hides its alts, every answer about one of its characters is the
   answer about a character nobody linked: `characters` names none, `befriend` is `ok`, and
   another person's view lists that character as a handle alone.
3. **A view of a character whose person hides its alts** names none of the person's other
   characters and no person friend added from one of them.
4. **An unlinked character** is at once, to every answer, a character nobody linked: its former
   person's views no longer name it, and other persons' views list it as a handle alone. Its own
   view names the friends handed back to it once: only those that held for it, which, while its
   person hid its alts, were those added from it alone.
5. **A world cannot show a person's characters.** A world speaks for its characters, and its
   operator could speak falsely for one: so a world may hide its character's person's characters,
   never show them, and nothing it asks for one character is answered from what the person's
   other characters hold while they are hidden.
6. **The person itself** is shown to nobody but itself, signed in, and `GET /v1/person` shows its
   name, not its Discord id. A world never learns either. The hub's operator, who keeps the
   registry, can read what section 5.1 lists there (the command line's `operators` lists accounts,
   not their characters).
7. **No log** holds a code, and a person's events hold no character's address; the wire log names
   message types only.

**A known limit: changes on the site arrive together.** Some of a person's own changes on the site
change the views of several of its characters at once: forgetting the person changes every one,
and removing a person friend added from several of its characters changes each of theirs. The hub
sends those views together, so a world holding two of the person's characters, or worlds
comparing what they received, could take the characters for one person even while it hides its
alts. Only the person causes it, signed in on the site: while the person hides its alts, nothing a
world asks for one of its characters changes the view of another.

### 5.6 Later: Discord beyond signing in

Specified as later work; not built. Each needs settings in the Discord application that its owner
makes. The first two also need the application's bot token, which only the hub would hold (worlds
would need no Discord code or secret, and would speak to the hub as they do now), and Discord's
approval for its communication features past their development limits:

- **Direct messages between the game and Discord**: a tell to a linked person's Discord account
  when none of its characters is online, and a Discord message to a character, through the hub,
  with the person's consent and under the same Reachable rules a tell has.
- **Channels carried into Discord**: the Global room or a channel bridged both ways with a lobby of
  Discord's Social SDK, or with a channel of a Dereth Network Discord server; a Discord user would
  be a speaker at a reserved world (`Name@discord`).
- **Rich presence**: Discord showing what a linked person plays and on which world, set by the
  Dereth client or a launcher on the player's own machine, which know both.

None of it may change section 5.5: a person's characters and Discord account stay hidden while
its alts are.
