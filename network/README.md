# The Dereth Network

The link between Asheron's Call worlds: cross-world tells, friends with online status, a shared
Global room and invite-only channels, for players on any client, the unmodified final retail client
included, through their own world's server. The specification is [`docs/network/`](../docs/network/README.md).

This folder is self-contained so that it can become a repository of its own: nothing in it
depends on a crate outside it, and its crates name their own dependency versions.

| folder | what | licence |
|---|---|---|
| [`protocol/`](protocol) | `dereth-network-protocol`: addresses, settings, the text rules, stand-in ids, rate limits, the link messages and the directory's, with no I/O; builds for `wasm32-unknown-unknown` | MIT, and the Unicode License v3 for the Windows best-fit table it embeds |
| [`hub/`](hub) | `dereth-network-hub`: the hub — the world registry (registration, `drth_` keys, claims, heartbeats, the community-list import, the federation flag), the operator API (Discord sign-in, verification, approval, reports), persons (characters linked to a Discord account with one-time links, person friends), the character directory (listed characters' profiles, published as their worlds send them) and the router for the worlds' links | AGPL-3.0-only |
| [`conformance/`](conformance) | `dereth-network-conformance`: the vectors, the hub checks and the world checks | MIT |
| [`ace-mod/`](ace-mod) | `Dereth.Network`, the ACE mod: the network for a stock ACE world | AGPL-3.0-only |

Empyrean's own network half is part of the server (`empyrean/crates/world/src/federation/` and
`empyrean/server/src/federation_link.rs`), configured by `[federation]` in `empyrean.toml`.

## Running a hub

```sh
cargo build --profile test-release -p dereth-network-hub
dereth-network-hub --db hub.sqlite register coldeve.json      # prints the world and its key, once
dereth-network-hub --db hub.sqlite federation coldeve on
dereth-network-hub --db hub.sqlite serve                        # http and ws on 127.0.0.1:8650
```

`dereth-network-hub --help` lists the rest: `update`, `list`, `claim`, `rotate-key`, `revoke-key`,
`import-community` (from a file or `--fetch`ed; a dry run unless `--apply`), `export-community`,
`audit`, `backup`, vouching for a world its operator cannot verify (`verify`), the report
queue and operators (`reports`, `report`, `unlock`, `assign`, `operators`, `disable-operator`), and
`self-update`, which installs a newer release over the binary. The hub listens on loopback unless `--listen` names another address; put TLS
in front of it with a reverse proxy.

World operators can register, verify and manage their worlds themselves through the hub's operator
API once it is given a Discord application (`--public-url`, `--site-origin`,
`--discord-client-id`, and the secret in `DERETH_HUB_DISCORD_CLIENT_SECRET`); the site calls it
from the browser. The same sign-in lets players link their characters: a world asks the hub for a
one-time code with `/dn link`, and the player enters it on the first site origin's `/characters/` page.
A hub without sign-in links no character. Given the application's bot token as well
(`--discord-bot-token-file`), the hub is the Discord broker: it mirrors rooms into Discord
servers' channels, the Global room and network channels as its admins set on the site, and a
world's own rooms, and the Global room, as the world's operator sets there; carries a tell to a
character into its person's Discord direct messages while the person does not play it, if the
person chose that on the site, and the person's reply back; lets a person speak in a world's
mirror as one of its own linked characters, through that world, with the bot's `/dn say`; no
world holds a Discord secret. [`hub/DEPLOY.md`](hub/DEPLOY.md) is the guide to running a public hub: the
reverse proxy, Cloudflare, systemd, the container, backups and updates. [`RELEASING.md`](RELEASING.md) says
how the hub and the ACE mod are released, on tags of their own.

## Linking an Empyrean world

In `empyrean.toml`:

```toml
[federation]
enabled = true
hub_url = "wss://hub.example.org"   # or ws://127.0.0.1:8650 for a hub on the same machine
slug = "coldeve"
key_file = "network.key"            # holds the drth_ key the hub printed
```

The rooms are the hub's to keep, not the world's settings: which of the world's chat rooms carries
the Global room (Roleplay until its operator moves it), and which of its own rooms are mirrored into
Discord. The operator sets both on the site (`PUT /v1/operator/worlds/<slug>/global-room`,
`PUT /v1/operator/worlds/<slug>/discord/mirrors/general`), or the first in game with
`/dn admin global <room>`, and the hub tells the world at once.

With `enabled = true`, a setting the world cannot serve (a bad slug or hub address, an unknown room,
no key in the key file, a store it cannot read) stops the server at start-up with the reason, as
any other setting that does not fit does.

An Admin verifies the world with `/dn verify <token>` (the token the site gives): the hub sends a
small challenge to the world's game port, which must be reachable from the internet as players
need it to be, and the world posts it back over HTTPS from whatever address its calls leave by.

## Checking an implementation

```sh
dereth-network-conformance vectors
dereth-network-conformance hub --url ws://127.0.0.1:8650 --world aa=drth_... --world bb=drth_...
```

[`docs/network/06-conformance.md`](../docs/network/06-conformance.md) lists every check.
