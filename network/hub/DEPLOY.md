# Deploying the hub

How to run `dereth-network-hub` as a public service: behind a reverse proxy that holds TLS, behind
Cloudflare, under systemd or in a container, with its registry backed up and its releases installed
as they come. The behaviour this
relies on is specified in [`docs/network/05-hub.md`](../../docs/network/05-hub.md) (the operator
API, client addresses behind proxies, the link's keep-alive).

Two ways to put the hub behind Cloudflare. Choose one before section 3, since the service's flags
differ:

- **A Cloudflare tunnel (simplest).** `cloudflared` on the hub's machine connects out to Cloudflare
  and hands it requests on loopback. No open port, no certificate on the machine, and no list of
  Cloudflare's addresses to keep current. Follow sections 1-3 with the tunnel's flags, then section
  4's last part.
- **A reverse proxy of your own (Caddy or nginx) behind Cloudflare.** For a machine that already
  runs one. The hub must then know Cloudflare's address ranges, and section 3's script keeps them
  current.

```
 browser (the site, https://dereth.network) --+
 world (wss:// link, https:// verify) --------+--> Cloudflare --> cloudflared ------------> hub 127.0.0.1:8650
                                              |                  (a tunnel out to it)
                                              +--> Cloudflare --> Caddy or nginx :443 --> hub 127.0.0.1:8650
                                                   (TLS, proxy)    (TLS to the origin)    (plain HTTP and ws://)
```

- The hub is **`https://hub.dereth.network`**: the HTTP API, the operator API and the worlds'
  links (`wss://hub.dereth.network/v1/servers/<slug>/link`). The site is
  **`https://dereth.network`**, static pages that call the hub from the browser.
- The hub listens on **loopback** and speaks plain HTTP; TLS ends at Cloudflare (a tunnel) or at
  the reverse proxy on the same machine. Nothing but `cloudflared` or the proxy reaches the hub.
- The hub keeps one SQLite file, the registry, and nothing else on disk.

## 1. The binary

From a release (`network-hub-v*` on the repository's releases page; check the archive against
`SHA256SUMS`), or from source:

```sh
cargo build --release --locked -p dereth-network-hub
install -m 0755 target/release/dereth-network-hub /usr/local/bin/
```

`dereth-network-hub --help` lists every command and flag. Later releases can install themselves
over this binary (section 9).

## 2. Discord, for operators signing in

World operators sign in with Discord, which the hub does itself. In the Discord developer portal,
make an application, and under **OAuth2**:

- add the redirect `https://hub.dereth.network/v1/auth/discord/callback` (the hub's public URL and
  `/v1/auth/discord/callback`);
- note the **client id** (it goes on the command line) and reset the **client secret**, which goes
  only in a file only root reads (section 3), never on a command line, in a repository, or in a
  shell's history.

The hub asks for the `identify` scope only.

**The Discord broker** (optional: rooms mirrored into Discord servers' channels, tells into persons'
Discord, and lines persons say from Discord) needs the application's **bot token**: under
**Bot**, reset the token, and keep it as the client secret is kept, in a file only root reads
(section 3), never on a command line, in a repository, or in a shell's history. The hub alone holds
it; no world does.

## 3. The service (systemd)

A user of its own, a state directory, and a configuration directory the hub's group reads and
only root writes, so the hub can read its settings and never change them. The Discord secret is
readable by root alone; systemd hands it to the service (`LoadCredential=` below), and the hub never
sees the file itself:

```sh
useradd --system --home /var/lib/dereth-network-hub --shell /usr/sbin/nologin dereth-hub
install -d -o root -g dereth-hub -m 0750 /etc/dereth-network-hub
install -o root -g root -m 0600 /dev/null /etc/dereth-network-hub/discord-client-secret
# Then put the secret from the developer portal in it, alone on its line.
```

**With a tunnel, skip the rest of this paragraph and the script below**, and in the unit use these
flags in place of the three proxy lines (`--trusted-proxy 127.0.0.1 --trusted-proxy ::1`,
`--trusted-proxies-file ...` and `--proxy-hops 2`):

```
    --trusted-proxy 127.0.0.1     --client-address-header cf-connecting-ip ```

**With your own proxy:** Cloudflare's address ranges, which the hub trusts to name the client
(`/etc/dereth-network-hub/cloudflare.txt`, one range to a line), come from one script that
fetches them, checks them, replaces the file only when every line is a range (a failed or odd
download keeps the old file), and then makes the proxy's and the firewall's rules from that same
file, so the three never disagree. `/usr/local/sbin/dereth-hub-cloudflare`:

```sh
#!/bin/sh
# Refreshes Cloudflare's ranges for the hub, Caddy and ufw, from one file.
set -eu
dir=/etc/dereth-network-hub
new=$(mktemp "$dir/cloudflare.txt.XXXXXX")
trap 'rm -f "$new"' EXIT
# Both lists, or nothing: a failed fetch stops the script here.
v4=$(curl -fsS --max-time 30 https://www.cloudflare.com/ips-v4)
v6=$(curl -fsS --max-time 30 https://www.cloudflare.com/ips-v6)
printf '%s\n%s\n' "$v4" "$v6" | grep -v '^$' > "$new"
# Every line an address range, both families there, and enough of them, or the old file stays.
if grep -Evq '^[0-9A-Fa-f:.]+/[0-9]{1,3}$' "$new" || ! grep -q '\.' "$new" || ! grep -q ':' "$new" \
    || [ "$(wc -l < "$new")" -lt 10 ]; then
    echo "Cloudflare's ranges did not read; keeping the old ones" >&2
    exit 1
fi
chown root:dereth-hub "$new"
chmod 0640 "$new"
mv -f "$new" "$dir/cloudflare.txt"
trap - EXIT

# Caddy's trusted_proxies (section 4 imports this file).
printf 'trusted_proxies static %s\n' "$(tr '\n' ' ' < "$dir/cloudflare.txt")" > /etc/caddy/cloudflare.caddy
systemctl reload caddy
# ufw: only Cloudflare reaches port 443 (section 5).
ufw status numbered | awk -F'[][]' '/# cloudflare/ { print $2 }' | sort -rn \
    | while read -r n; do ufw --force delete "$n"; done
while read -r range; do ufw allow proto tcp from "$range" to any port 443 comment cloudflare; done \
    < "$dir/cloudflare.txt"
systemctl restart dereth-network-hub
```

With your own proxy, run it once now (`sh /usr/local/sbin/dereth-hub-cloudflare`, as root);
section 6's timer runs it weekly.

`/etc/systemd/system/dereth-network-hub.service`:

```ini
[Unit]
Description=Dereth Network hub
After=network-online.target
Wants=network-online.target

[Service]
User=dereth-hub
Group=dereth-hub
StateDirectory=dereth-network-hub
StateDirectoryMode=0750
# The secret, handed over by systemd: %d is the service's own credentials directory.
LoadCredential=discord-client-secret:/etc/dereth-network-hub/discord-client-secret
Environment=DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite
ExecStart=/usr/local/bin/dereth-network-hub serve \
    --listen 127.0.0.1:8650 \
    --trusted-proxy 127.0.0.1 --trusted-proxy ::1 \
    --trusted-proxies-file /etc/dereth-network-hub/cloudflare.txt \
    --proxy-hops 2 \
    --public-url https://hub.dereth.network \
    --site-origin https://dereth.network \
    --discord-client-id 000000000000000000 \
    --discord-client-secret-file %d/discord-client-secret \
    --admin 000000000000000000
Restart=on-failure
RestartSec=5
# The hub needs its state directory and the network; nothing else.
UMask=0077
NoNewPrivileges=true
CapabilityBoundingSet=
ProtectSystem=strict
ProtectHome=true
ProtectProc=invisible
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictNamespaces=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
LockPersonality=true
MemoryDenyWriteExecute=true
SystemCallArchitectures=native
SystemCallFilter=@system-service

[Install]
WantedBy=multi-user.target
```

Before Discord is set up (section 2), leave out the `LoadCredential=` line and the three Discord
flags (`--discord-client-id`, `--discord-client-secret-file`, `--admin`): systemd refuses to start a
service whose credential file is missing. Sign-in then reads as not open yet, and everything else
works. Put them back, with the secret in its file, when Discord is ready.

For the Discord broker, put the bot token in a file of its own as the secret is
(`install -o root -g root -m 0600 /dev/null /etc/dereth-network-hub/discord-bot-token`, then the
token alone on its line), and add to the unit
`LoadCredential=discord-bot-token:/etc/dereth-network-hub/discord-bot-token` and the flag
`--discord-bot-token-file %d/discord-bot-token`. The log then says `Discord broker on`;
`curl -s http://127.0.0.1:8650/v1/config` shows what the broker does. The bot needs no privileged
intent: it reads nothing in servers, and Discord gives it its own direct messages.

**Tells into persons' Discord.** With sign-in and the bot token, a tell to a linked character
reaches its person's Discord while the person does not play that character: the bot says each in
its direct messages, and the person's reply goes back as a tell from that character. There is no
setting for it. Discord lets the bot message only a user who shares a server with it and lets that
server's members message them, so add the bot to the network's own Discord server (it needs no
permission there) and tell players to join it; a person Discord refuses is not tried again for an
hour. The bot reads a person's direct messages every `--discord-poll-secs` seconds (3) while a
reply is awaited. The counts are at `GET /v1/admin/discord` under `tells`.

**Mirrors into Discord servers' channels.** A mirror says a room's lines in one channel of a Discord
server, under each speaker's address. Discord users' lines there stay in Discord; a person speaks
into a world operator's mirror only as one of its own linked characters, with the bot's `/dn say`
(below). An admin (`--admin`) mirrors the Global room, and network channels, on the site's admin
page; each world's operator mirrors its own rooms, and the Global room, into a server it runs, on
the operator pages. Nothing is set on the command line, and changes take effect at once. A mirror
goes either way:

1. **Through the bot.** Add the bot to the server at
   `https://discord.com/oauth2/authorize?client_id=<client id>&scope=bot%20applications.commands&permissions=0`
   (the site shows it as `invite`): the scopes `bot` and `applications.commands` (the `/dn`
   command) and no permissions. A server that added the bot before with the scope `bot` alone adds
   it again at this address to get the command. Then, on the mirror's channel alone
   (Edit Channel, Permissions, add the bot), allow it View Channel and Manage Webhooks, so it sees no
   other channel beyond what any member sees. In Discord, turn on Developer Mode (User Settings,
   Advanced), right-click the channel and Copy Channel ID, and give that id on the site. The hub
   refuses a channel the bot cannot see or lacks either permission in, and one the asker may not
   manage webhooks in. The log then says `made the Discord webhook ... for global in the channel
   ...` (after a restart, that the webhook mirrors it again): the bot keeps one webhook in the
   channel, named Dereth Network, and says each line through it.
2. **Through a webhook of your own**, with the bot nowhere in the server: in the channel's settings,
   Integrations, Webhooks, make a webhook, copy its URL, and give that on the site. The hub keeps the
   URL secret: it is never logged or shown again. Should the webhook be deleted, the log says the
   mirror needs another. Such a mirror only receives: without the bot, the server has no `/dn`.

A webhook has Discord's own limits; a line past them is held back, counted at `GET
/v1/admin/discord` under the mirror's `lines`, and the log sums up each hour. The picture the lines
show is set on the admin page, as is taking away any world operator's mirror, which is how the
network moderates the Global room's mirrors in other servers.

**Speaking from Discord.** In a world operator's mirror, a person who linked characters on that
world chooses one with `/dn identity` and says a line as it with `/dn say <text>`; the hub asks the
character's world to say it, and the world's line then reaches the other worlds and every mirror,
marked as said from Discord (`Ann@coldeve (via /dn)` in the mirrors). The admins' mirrors are read
only. Discord sends each use of the command to the hub, so, once the hub runs with sign-in and the
bot token, set the application's **Interactions Endpoint URL** in the developer portal (General
Information) to the hub's public address and `/v1/discord/interactions`
(`https://hub.dereth.network/v1/discord/interactions`), and save: Discord checks the address there
and then, so the hub must be running and reachable. The hub reads the application's public key
with the bot token at its start, and registers the `/dn` command (the log says `the /dn command is
registered`); it opens no Gateway connection and needs no privileged intent. It refuses any request
there that Discord did not sign within five minutes of its own clock, so keep the host's clock set
(NTP). Every answer is seen by the person alone. A world says such lines once it runs a version of
Empyrean or the ACE mod that knows them; until then the person is told the world did not answer.

```sh
systemctl daemon-reload
systemctl enable --now dereth-network-hub
journalctl -u dereth-network-hub -f
```

- `--admin` is the hub operator's own Discord id: it may use the admin routes (the report queue)
  from the site, and register worlds at names under `dereth.network` (your own test worlds, say)
  from the site's operator pages. Repeat it for each admin. An admin is held to none of one
  operator's own allowances (registrations, worlds and claims held, claims, verification tokens
  and attempts, page checks, approvals, a world's new keys and changes): only to its address's
  request allowance, the hub's ceilings and five reports a day. The command line needs no sign-in
  at all, and registers there too.
- **Names under the domain.** No world's game endpoint may be `dereth.network`,
  `hub.dereth.network`, `api.dereth.network`, or the host of `--public-url` or a `--site-origin`,
  whoever registers it. Any other name under `dereth.network` is yours alone: refused for every
  operator but an `--admin`, and for the community import. A world's public links point at
  `--public-url` and the first `--site-origin`; the command line takes both flags (or
  `DERETH_HUB_PUBLIC_URL` and `DERETH_HUB_SITE_ORIGINS`), and assumes `https://hub.dereth.network`
  and `https://dereth.network` without them.
- **The command line** (`reports`, `unlock`, `remove`, `backup`, and the rest) works on the same
  registry file, so run it as the hub's user, never as root: a file root makes beside the registry
  (its write-ahead log) would lock the hub out of it. `self-update`, which replaces the binary and
  only reads the registry, is root's (section 9).

  ```sh
  sudo -u dereth-hub DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite dereth-network-hub reports
  ```
- The log names requests' kinds, worlds and refusals; never a key, a token, a session, the secret
  or a message's text.
- **The hub's settings** are kept in the registry, and an admin changes them on the site's admin
  page (Hub settings, on the operators page); a change takes effect at once, with no restart:
  - the ceiling on worlds joining the network a day on their own, all operators together (500;
    0 to 100,000); each operator's own allowance (3 a day) is what binds;
  - how old an operator's Discord account must be to register or claim a world (30 days; 0 to
    3,650);
  - how long a registered world may wait to be verified before it is removed and its slug freed
    (14 days; 1 to 365).

  **Hubs before these were kept in the registry** (the 0.1 releases) set them with the flags
  `--approvals-per-day`, `--min-account-age-days` and `--pending-days`. A newer hub still takes
  them, but only to seed the registry: a flag given while the registry holds no value of its
  setting becomes the registry's value, and the log says so. After that the registry's value holds,
  and a flag that differs is ignored, with a line in the log naming both values
  (`--approvals-per-day 400 is ignored: the registry holds 300 ...`). Once the log says a flag
  seeded its setting, take the flag out of the unit: the flags are removed in a later minor release,
  and a hub refuses to start with a flag it does not know.
- `remove <slug>` removes any world for good; `unlist <slug>` and `relist <slug>` take one out of
  the public lists and put it back (run them as the hub's user, below).
- `--deny-fetch` (repeated, or one range to a line in a `--deny-fetch-file`): ranges the hub never
  reads a verification page from, and never sends a verification challenge to, whatever else
  allows them. The hub already refuses private and reserved addresses; list what is public but
  yours: the basement's WAN address, its IPv6 prefix (the whole /56 or /64 the ISP gives it), and
  the LAN, so that no operator's page or endpoint can make the hub reach the router or anything
  behind it:

  ```
  --deny-fetch 203.0.113.7/32 --deny-fetch 2001:db8:1234::/48 --deny-fetch 192.168.1.0/24
  ```

  Where the ISP changes the address, keep the file current (a cron job writing the address). For
  more separation, send the hub's outbound requests out through another egress (a VPN or a small
  VPS proxy for the hub's user only), so a page check never leaves from the basement's address.
- **Verification in game sends UDP.** When a world's operator types `/dn verify`, the hub sends a
  few small datagrams (at most twelve for an attempt) from a port the system picks to the world's
  game port, and waits for the world to post them back over HTTPS. The machine's firewall must let
  the hub's user send UDP out; nothing comes back over UDP, so no port is opened for it. The world
  may call from any address: what it proves is that it answers at its endpoint.
- **Vouching for a world.** A world that can be verified neither in game (its game port cannot be
  reached from where the hub is) nor by a page can be verified by you, once you have seen evidence
  that its operator runs it at its endpoint:

  ```sh
  sudo -u dereth-hub DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite \
    dereth-network-hub verify coldeve "Saw its console and its game port on a call with its operator."
  ```

  The world is verified by the method `admin`, its owner's token is spent and approval is asked as
  a proof asks it; the evidence goes in the world's log, where its owner sees it as the hub
  operator's. A community listing is taken with `claim` instead, and a locked world is refused.

## 4. The reverse proxy

The proxy holds TLS, holds each request until its head has arrived (both proxies below do), passes
WebSocket upgrades on, and tells the hub who the client is. The hub believes only the proxies it is
told to trust (section 3's `--trusted-proxy` lines and the Cloudflare list), and reads
`X-Forwarded-For` from the right, one entry for each proxy a request crosses: `--proxy-hops 2` for
Cloudflare and then this proxy, `--proxy-hops 1` (the default) for this proxy alone. Each proxy
appends the address it was reached from, so the entry two from the right is the one Cloudflare's
edge was reached from, and nothing a client (or a Cloudflare Worker) writes further left is ever
read. A header that does not read is refused with a `400`.

### Caddy

`/etc/caddy/Caddyfile`:

```caddy
{
	servers {
		# Cloudflare's edge: Caddy keeps the X-Forwarded-For it sends, and appends the edge's
		# own address. The file is made from cloudflare.txt by section 3's script.
		import /etc/caddy/cloudflare.caddy
	}
}

hub.dereth.network {
	# A Cloudflare origin certificate (SSL/TLS mode Full (strict)), or leave this line out and let
	# Caddy obtain a public certificate.
	tls /etc/caddy/hub-origin.pem /etc/caddy/hub-origin-key.pem
	request_body {
		max_size 1MB
	}
	reverse_proxy 127.0.0.1:8650
}
```

Caddy passes WebSocket upgrades on by itself and sets no idle timeout on a proxied stream.

### nginx

```nginx
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;
    server_name hub.dereth.network;
    ssl_certificate     /etc/nginx/hub-origin.pem;
    ssl_certificate_key /etc/nginx/hub-origin-key.pem;
    client_max_body_size 1m;
    # Heads as large as Cloudflare passes on, so the hub answers one too large itself (431), in
    # words the site can read, where nginx's own limit (8 KiB a line) would answer it bare.
    large_client_header_buffers 4 32k;

    location / {
        proxy_pass http://127.0.0.1:8650;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        # Appends the address nginx saw (Cloudflare's edge) to what Cloudflare sent.
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        # nginx closes a proxied connection idle for 60 seconds by default: longer than the
        # link's 20-second ping is enough, and this leaves room.
        proxy_read_timeout 120s;
        proxy_send_timeout 120s;
    }
}
```

### Without a proxy of your own: a Cloudflare tunnel

`cloudflared` on the same machine, with the tunnel's public hostname `hub.dereth.network` pointed at
`http://127.0.0.1:8650`, needs no open port and no certificate. Nothing but Cloudflare then reaches
the hub, so Cloudflare's own header is safe to read. To set it up on Debian or Ubuntu (Cloudflare
also publishes `.rpm` packages and a static binary):

```sh
mkdir -p --mode=0755 /usr/share/keyrings
curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | tee /usr/share/keyrings/cloudflare-main.gpg >/dev/null
echo 'deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main'     > /etc/apt/sources.list.d/cloudflared.list
apt-get update && apt-get install cloudflared
```

Then, in Cloudflare's dashboard (Zero Trust, Networks, Tunnels): create a tunnel of the
`cloudflared` kind, run the install command it shows on the machine (`cloudflared service install
<token>`, which makes it a system service), and give the tunnel a public hostname: `hub` on
`dereth.network`, service **HTTP** at **`127.0.0.1:8650`**. Name the address, not `localhost`, which
may resolve to `::1` and reach the hub from an address it does not trust. Cloudflare makes the DNS
record itself (remove an older record for the name first). The hub runs with section 3's tunnel
flags:

```
dereth-network-hub serve --listen 127.0.0.1:8650 --trusted-proxy 127.0.0.1 \
    --client-address-header cf-connecting-ip ...
```

The hub reads `CF-Connecting-IP` only from a peer it trusts (here `cloudflared` on loopback). Never
use `cf-connecting-ip` where anything but Cloudflare can reach the proxy that hands it on: anyone
could write that header.

## 5. Cloudflare

For `hub.dereth.network`:

- **DNS:** an `A` (and `AAAA`) record for `hub` to the basement's public address, **proxied**
  (orange cloud).
- **SSL/TLS:** mode **Full (strict)**, with a Cloudflare origin certificate (or a public one) on
  the proxy; **Always Use HTTPS** on; minimum TLS 1.2.
- **Network:** **WebSockets** on.
- **Timeouts.** Cloudflare closes a proxied connection that has been idle for **100 seconds**, and
  gives up on an answer that has not begun within 100 seconds (a 524). The hub pings every link
  every 20 seconds and closes one it has heard nothing from for 70, and the ACE mod pings the hub
  every 20 seconds too, so a quiet link is never cut. Nothing else the hub answers takes more than a
  few seconds (reading a verification page gives up after 10) but a world's verification in game,
  which the hub holds for up to 25 seconds (a lookup of up to 10, then up to 15 for the world's
  answer to its challenge): a proxy of your own must wait at least that long for an answer (both
  below do).
- **Caching:** a Cache Rule for `hub.dereth.network` that caches only what the hub marks as
  cacheable ("Respect origin" / "Use cache-control header if present"), so the public lists
  (`/v1/servers`, `/v1/status`, `/v1/network`, `/compat/*`, sent with
  `Cache-Control: public, max-age=10`) are kept ten seconds at the edge, the character directory's
  answers (`/v1/directory/*`, `public, max-age=60`) a minute, and everything else
  (`Cache-Control: no-store`), the operator API, the person routes and the links above all, never
  is. Do not use "Cache everything": a character taken out of the directory would linger.
- **Security:** no JavaScript challenge, *I'm Under Attack* mode or Bot Fight Mode on the hub:
  world servers and the site's calls cannot answer a challenge. A WAF rate-limiting rule is fine
  for `/v1/auth/*` and `/v1/operator*`; leave `/v1/servers/*/link`, `/v1/servers/*/verify` and
  `/v1/servers/*/verify/answer` alone (the hub limits them itself).
- **The origin** (your own proxy; a tunnel opens no port, and needs neither this nor the next
  point). Let only Cloudflare reach port 443 at home (the router, or a firewall on the
  machine), from the same ranges as `cloudflare.txt`: section 3's script keeps `ufw`'s rules (each
  commented `cloudflare`) to that file, with `ufw default deny incoming` and SSH allowed from your
  own network only. On a router, copy the same file's ranges.

- **Authenticated Origin Pulls (recommended).** Turn it on with a **zone-level (per-hostname)
  certificate** you upload, not Cloudflare's shared one, and have the proxy demand it (Caddy's
  `client_auth { mode require_and_verify; trusted_ca_cert_file ... }`, nginx's
  `ssl_verify_client on`). Then only your own zone's traffic reaches the proxy: another Cloudflare
  customer's Worker, which also comes from Cloudflare's ranges, cannot.
- **Checking that a Worker cannot choose its address** (after deploying; it needs a Cloudflare
  account of your own, any one):
  1. Make a Worker that fetches `https://hub.dereth.network/v1/servers` with the header
     `X-Forwarded-For: 192.0.2.1` and returns the status.
  2. Call it a few hundred times in a minute, faster than the hub's per-address allowance (180).
  3. The calls should be held back with `429` once the allowance is spent, as one address's
     would: the Worker is counted under the Cloudflare address it came from, never `192.0.2.1`.
     With Authenticated Origin Pulls on, the proxy should refuse the Worker's requests outright.

The site at `dereth.network` calls the hub from the browser, so its Content-Security-Policy allows
`connect-src https://hub.dereth.network`, and its sign-in link is
`https://hub.dereth.network/v1/auth/discord?return=<path>`. Players link characters on its
`/link/` page (the first `--site-origin` and `/link/`, which the hub names, with the code after a
`#`, in every link it gives a world), with the same sign-in, and see them on `/characters/`; nothing
else needs configuring for it.

## 6. Backups

The registry is one SQLite file in write-ahead mode. `dereth-network-hub backup <file>` writes a
consistent copy of it while the hub runs (one transaction's view); copying the file itself while
the hub runs is not safe.

`/etc/systemd/system/dereth-network-hub-backup.service`:

```ini
[Unit]
Description=Dereth Network hub: back up the registry

[Service]
Type=oneshot
User=dereth-hub
Environment=DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite
ExecStart=/bin/sh -c 'dereth-network-hub backup /var/backups/dereth-network-hub/hub-$(date -u +%%Y%%m%%dT%%H%%M%%SZ).sqlite'
ExecStartPost=/usr/bin/find /var/backups/dereth-network-hub -name 'hub-*.sqlite' -mtime +30 -delete
```

`/etc/systemd/system/dereth-network-hub-backup.timer`:

```ini
[Unit]
Description=Dereth Network hub: back up the registry daily

[Timer]
OnCalendar=daily
RandomizedDelaySec=1h
Persistent=true

[Install]
WantedBy=timers.target
```

```sh
install -d -o dereth-hub -g dereth-hub -m 0700 /var/backups/dereth-network-hub
systemctl enable --now dereth-network-hub-backup.timer
```

With your own proxy, section 3's script refreshes Cloudflare's ranges weekly, beside it (a tunnel
needs neither):
`/etc/systemd/system/dereth-hub-cloudflare.service` runs `/bin/sh /usr/local/sbin/dereth-hub-cloudflare`
(`Type=oneshot`, as root), and `/etc/systemd/system/dereth-hub-cloudflare.timer` has
`OnCalendar=weekly`, `RandomizedDelaySec=6h` and `Persistent=true`. A refresh that fails leaves
the old ranges in place and says so in its log (`journalctl -u dereth-hub-cloudflare`).

- **Copy the backups off the machine.** They hold the worlds, their keys' digests (never the keys),
  the operators' Discord ids and names, the reports, the channels and the audit log: keep them as
  private as the machine.
- **Restoring:** stop the hub, remove `hub.sqlite-wal` and `hub.sqlite-shm`, copy the backup over
  `hub.sqlite` (owned by `dereth-hub`), start the hub. Everything goes back to the backup's moment:
  - keys issued after the backup no longer work; their worlds get new ones from the site or
    `dereth-network-hub rotate-key`;
  - sessions come back as they were: a sign-out or a disabled operator's ended sessions since the
    backup are live again until they lapse. End them all before starting the hub
    (`sudo -u dereth-hub DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite dereth-network-hub end-sessions`);
    everyone signs in again;
  - locks and unlocks, and operators disabled or enabled, since the backup are undone: do them
    again (`report`, `unlock`, `disable-operator`, `enable-operator`), from your own notes or the
    old file's audit log, before anyone else is let back in;
  - worlds registered, removed or lapsed since the backup are as they were then.
- **Upgrading:** take a backup first (`self-update --apply`, section 9, takes one itself). A newer
  hub moves the registry's schema forward when it opens it; an older hub refuses to start on a
  registry newer than itself, so a downgrade needs the backup.
  - From 0.1.x (schema 7), or from a pre-release build at schema 8, the move is automatic: the
    first start of this hub brings the registry to its schema (`--registry-schema`), keeping every
    world, account, session, link, friend, listing and mirror. Only what the network no longer
    has goes: each person's two settings, and the holds persons put on their characters'
    listings.

### The community list, daily

The hub can keep the community server list's worlds listed as unclaimed listings by importing the
list once a day (with a tunnel or a proxy of your own alike). The import reads the list from the
community repository, applies everything but conflicts, and takes out of the public lists a world
the list has dropped for three days running (never deleting it, and never touching a world an
operator manages); a world that returns is listed again. A list that does not arrive changes
nothing. It works on the hub's registry while the hub runs, as the hub's user.

Run it once by hand first, as a dry run, and read the plan:

```sh
sudo -u dereth-hub DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite dereth-network-hub import-community --fetch
```

`/etc/systemd/system/dereth-network-hub-community.service`:

```ini
[Unit]
Description=Dereth Network hub: import the community server list
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
User=dereth-hub
Group=dereth-hub
StateDirectory=dereth-network-hub
StateDirectoryMode=0750
Environment=DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite
ExecStart=/usr/local/bin/dereth-network-hub import-community --fetch --apply --skip-conflicts --summary
UMask=0077
NoNewPrivileges=true
CapabilityBoundingSet=
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
PrivateDevices=true
RestrictNamespaces=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
SystemCallFilter=@system-service
```

`/etc/systemd/system/dereth-network-hub-community.timer`:

```ini
[Unit]
Description=Dereth Network hub: import the community server list daily

[Timer]
OnCalendar=daily
RandomizedDelaySec=1h
Persistent=true

[Install]
WantedBy=timers.target
```

```sh
systemctl daemon-reload
systemctl enable --now dereth-network-hub-community.timer
journalctl -u dereth-network-hub-community
```

Each run logs one line: how many worlds were added, updated, unchanged, in conflict, unlisted and
relisted, naming the ones in conflict, unlisted or relisted. A conflict (a slug another world
holds, or an address under the hub's own domain or at one of its names) needs you: settle it by hand, and the next run
imports the world. `--retire-after <n>` changes the three days (0 never takes a world out);
`relist <slug>` puts back a world the import took out, and the count starts again.

## 7. The container

`network/hub/Dockerfile` puts the release's Linux binary on a distroless base (glibc, no shell),
running as an unprivileged user. Each release also carries the image, for `linux/amd64` and
`linux/arm64`. In a container the hub listens on all addresses, and the proxy reaches it over a
network of their own. The hub trusts the proxy's one address on it, never a whole range (not
Docker's `172.16.0.0/12`, which every other container on the machine is in too): give the network
a subnet and the proxy a fixed address on it. The image's default command names no proxy, so the
hub will not start until it is told one.

```sh
docker network create --subnet 172.31.86.0/24 dereth-proxy
# The proxy's container joins it with: --network dereth-proxy --ip 172.31.86.2
docker volume create dereth-network-hub
docker run -d --name dereth-network-hub --restart unless-stopped \
    --network dereth-proxy -v dereth-network-hub:/data \
    -v /etc/dereth-network-hub/cloudflare.txt:/etc/dereth-network-hub/cloudflare.txt:ro \
    -v /etc/dereth-network-hub/discord-client-secret.65532:/run/secrets/discord-client-secret:ro \
    ghcr.io/dereth-network/dereth/network-hub:0.1.0 \
    serve --listen 0.0.0.0:8650 --trusted-proxy 172.31.86.2 \
    --trusted-proxies-file /etc/dereth-network-hub/cloudflare.txt --proxy-hops 2 \
    --public-url https://hub.dereth.network --site-origin https://dereth.network \
    --discord-client-id 000000000000000000 \
    --discord-client-secret-file /run/secrets/discord-client-secret
```

The secret's copy for the container is readable by the image's user alone
(`install -o 65532 -g 65532 -m 0400 /etc/dereth-network-hub/discord-client-secret
/etc/dereth-network-hub/discord-client-secret.65532`); Compose's or Swarm's `secrets:` do the same.

The registry is `/data/hub.sqlite` in the volume; back it up with
`docker exec dereth-network-hub dereth-network-hub backup /data/hub-backup.sqlite`, then copy the
file out.

To build the image yourself, lay the binaries out by architecture and build from that folder:

```sh
mkdir -p ctx/bin/amd64 ctx/data && cp target/release/dereth-network-hub ctx/bin/amd64/
docker build -f network/hub/Dockerfile -t dereth-network-hub ctx
```

## 8. Checking it

```sh
curl -fsS https://hub.dereth.network/healthz             # {"status":"ok",...}
curl -fsS https://hub.dereth.network/v1/config           # "authEnabled": true once Discord is set
curl -fsS https://hub.dereth.network/v1/network          # the worlds on the network, and which are linked
```

On a world, the ACE mod's log shows `connected to the hub` and `welcomed`; Empyrean's shows
`Dereth Network: connected to the hub`.

## 9. Updating

`dereth-network-hub self-update` replaces the binary it runs from with a newer release of the hub,
and nothing else: it starts and stops no service, so restart the hub afterwards. Run it as root,
who owns `/usr/local/bin`. The hub's own user cannot replace its binary, and the running hub never
replaces itself.

```sh
export DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite
dereth-network-hub self-update --check       # what would be installed, and why not the others
dereth-network-hub self-update --apply
systemctl restart dereth-network-hub
```

- **Which release.** The newest `network-hub-v<version>` release of the repository the binary was
  built from (`--source` names another, or a mirror of GitHub's releases API) that `--policy`
  allows:
  - **`--policy patch`**: this version's major and minor (`0.1.x` for `0.1.1`), and only a release
    whose binary reads the same registry schema as this one. Only the binary changes; the registry
    is left as it is, so going back is putting the old binary back. This is the one to automate.
  - **`--policy minor`** (the default): any release of this major version. The new hub moves the
    registry's schema forward when it first opens it, and an older hub then refuses to start on
    it, so going back needs the registry's copy as well. Run it by hand, after reading the
    release's notes.
  - Never a major release, a pre-release, or a release that failed here (below): install those by
    hand, as section 1 does.
- **What `--apply` does**, stopping at the first thing that fails, with the binary unchanged:
  1. downloads this platform's archive and the release's `SHA256SUMS`, and refuses the archive
     unless its SHA-256 is the one listed. That is the only check: releases are not signed, so it
     catches a damaged or altered download, not a release someone else published;
  2. runs the new binary, which must name the release's version (`--version`) and read a registry
     schema no older than the registry's (`--registry-schema`);
  3. copies the registry, as `backup` does, to `hub.sqlite.backup-update-v<old>-v<new>-<UTC time>`
     beside it, owned by the registry's owner and readable by it alone, and keeps the newest three
     such copies;
  4. moves the binary aside as `dereth-network-hub.previous-v<old>` (removing the one an earlier
     update left) and the new one into its place.

  It refuses before downloading anything when it cannot write in the binary's folder (naming the
  folder) or finds no registry (`--db`, or `DERETH_HUB_DB`, as for every command). It only reads
  the registry, and SQLite gives the files it makes beside it to the registry's owner, so running
  it as root never locks the hub out.
- **Exit status:** 0 when it installed a release (with `--check`, when one would be), 3 when there
  was none to install, 1 when it refused or failed.
- **A release that failed here** (its binary does not run on this machine, names another version,
  or reads an older schema than the registry's) is recorded in
  `/usr/local/bin/.dereth-network-hub-update.json` and not tried again until that file is removed.
  A download that does not match its checksum is not recorded; the next run downloads it again.
- **Going back:**

  ```sh
  systemctl stop dereth-network-hub
  mv /usr/local/bin/dereth-network-hub.previous-v0.1.1 /usr/local/bin/dereth-network-hub
  systemctl start dereth-network-hub
  ```

  After a patch release that is all. After a minor release that moved the registry's schema
  forward, the older hub refuses to start ("The registry's schema is newer than this hub"):
  restore the update's copy of the registry first, as section 6 restores a backup, and everything
  since the copy is lost.
- **In a container** the binary is the image's: run the new release's image instead.
  `self-update` cannot write in the image's folder, and says so.

### Automatic updates

Off until you turn them on. A daily timer runs `self-update --apply --policy patch` as root and,
when it has installed a release, restarts the hub if it is running. Use `patch` only: a patch
release changes the binary alone, and going back from it is moving the old binary back. A minor
release can move the registry's schema forward and is for an operator to install and watch.

`/etc/systemd/system/dereth-network-hub-update.service`:

```ini
[Unit]
Description=Dereth Network hub: install a new patch release
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
Environment=DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite
# Root replaces the binary; the hub's own user cannot. Exit status 0: a release was installed, so
# the hub restarts into it; 3: there was none to install.
ExecStart=/bin/sh -c 'if /usr/local/bin/dereth-network-hub self-update --apply --policy patch; then systemctl try-restart dereth-network-hub; else test $$? -eq 3; fi'
UMask=0077
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/usr/local/bin /var/lib/dereth-network-hub
ProtectHome=true
PrivateTmp=true
```

`/etc/systemd/system/dereth-network-hub-update.timer`:

```ini
[Unit]
Description=Dereth Network hub: install a new patch release daily

[Timer]
OnCalendar=daily
RandomizedDelaySec=1h
Persistent=true

[Install]
WantedBy=timers.target
```

```sh
systemctl daemon-reload
systemctl enable --now dereth-network-hub-update.timer
journalctl -u dereth-network-hub-update
```

Each run logs what it found and why it took or left each newer release; a refused release leaves
the unit failed (`systemctl --failed`), and the hub running as it was.
