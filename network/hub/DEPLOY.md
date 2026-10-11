# Deploying the hub

How to run `dereth-network-hub` as a public service: behind a reverse proxy that holds TLS, behind
Cloudflare, under systemd or in a container, with its registry backed up. The behaviour this
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

`dereth-network-hub --help` lists every command and flag.

## 2. Discord, for operators signing in

World operators sign in with Discord, which the hub does itself. In the Discord developer portal,
make an application, and under **OAuth2**:

- add the redirect `https://hub.dereth.network/v1/auth/discord/callback` (the hub's public URL and
  `/v1/auth/discord/callback`);
- note the **client id** (it goes on the command line) and reset the **client secret**, which goes
  only in a file only root reads (section 3), never on a command line, in a repository, or in a
  shell's history.

The hub asks for the `identify` scope only.

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

```sh
systemctl daemon-reload
systemctl enable --now dereth-network-hub
journalctl -u dereth-network-hub -f
```

- `--admin` is the hub operator's own Discord id: it may use the admin routes (the report queue)
  from the site, and register worlds at names under `dereth.network` (your own test worlds, say)
  from the site's operator pages. Repeat it for each admin. The command line needs no sign-in at
  all, and registers there too.
- **Names under the domain.** No world's game endpoint may be `dereth.network`,
  `hub.dereth.network`, `api.dereth.network`, or the host of `--public-url` or a `--site-origin`,
  whoever registers it. Any other name under `dereth.network` is yours alone: refused for every
  operator but an `--admin`, and for the community import. A world's public links point at
  `--public-url` and the first `--site-origin`; the command line takes both flags (or
  `DERETH_HUB_PUBLIC_URL` and `DERETH_HUB_SITE_ORIGINS`), and assumes `https://hub.dereth.network`
  and `https://dereth.network` without them.
- **The command line** (`reports`, `unlock`, `remove`, `backup`, and the rest) works on the same
  registry file, so run it as the hub's user, never as root: a file root makes beside the registry
  (its write-ahead log) would lock the hub out of it.

  ```sh
  sudo -u dereth-hub DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite dereth-network-hub reports
  ```
- The log names requests' kinds, worlds and refusals; never a key, a token, a session, the secret
  or a message's text.
- `--approvals-per-day` (default 500) is the ceiling on worlds joining the network a day on their
  own, all operators together; each operator's own allowance (3 a day) is what binds.
- `--min-account-age-days` (default 30) is how old an operator's Discord account must be to
  register or claim a world.
- `--pending-days` (default 14) is how long a registered world may wait to be verified before it is
  removed and its slug freed. `remove <slug>` removes any world for good; `unlist <slug>` and
  `relist <slug>` take one out of the public lists and put it back (run them as the hub's user,
  below).
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
  `Cache-Control: public, max-age=10`) are kept ten seconds at the edge, and everything else
  (`Cache-Control: no-store`), the operator API, the person routes and the links above all, never
  is. Do not use "Cache everything".
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
`/characters/` page (the first `--site-origin` and `/characters/`, which the hub names in every link
code it gives a world), with the same sign-in; nothing else needs configuring for it.

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
- **Upgrading:** take a backup first. A newer hub moves the registry's schema forward when it
  opens it; an older hub refuses a registry newer than itself, so a downgrade needs the backup.

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
