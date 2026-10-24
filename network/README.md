# The Dereth Network

The link between Asheron's Call worlds: cross-world tells, friends with online status, a shared
Global room and invite-only channels. It works on any client, the unmodified final retail client
included, through the player's own world's server. The specification is
[`docs/network/`](../docs/network/README.md).

This folder is self-contained so it can become a repository of its own: nothing in it depends on
a crate outside it, and its crates name their own dependency versions.

| Folder | What | License |
|---|---|---|
| [`protocol/`](protocol) | `dereth-network-protocol`: addresses, settings, the text rules, stand-in ids, rate limits and the link and directory messages. No I/O; builds for `wasm32-unknown-unknown`. | MIT, and the Unicode License v3 for the Windows best-fit table it embeds |
| [`hub/`](hub) | `dereth-network-hub`, the hub: the world registry, the operator API, linked characters, the character directory, the Discord broker and the router between worlds | AGPL-3.0-only |
| [`conformance/`](conformance) | `dereth-network-conformance`: the test vectors and the hub and world checks | MIT |
| [`ace-mod/`](ace-mod) | `Dereth.Network`, the mod that puts a stock ACE world on the network | AGPL-3.0-only |

Empyrean has the network built in (`empyrean/crates/world/src/federation/` and
`empyrean/server/src/federation_link.rs`), set up by `[federation]` in `empyrean.toml`.

## Running a hub

```sh
cargo build --profile test-release -p dereth-network-hub
dereth-network-hub --db hub.sqlite register coldeve.json      # prints the world and its key, once
dereth-network-hub --db hub.sqlite federation coldeve on
dereth-network-hub --db hub.sqlite serve                        # http and ws on 127.0.0.1:8650
```

`dereth-network-hub --help` lists the other commands: managing worlds and their keys, importing
and exporting the community server list, reports and operators, backups, and `self-update`.

The hub listens on loopback unless `--listen` names another address. Put TLS in front of it with a
reverse proxy. [`hub/DEPLOY.md`](hub/DEPLOY.md) covers running a public hub: the proxy, Cloudflare,
systemd, the container, backups and updates. [`RELEASING.md`](RELEASING.md) covers releasing the
hub and the ACE mod.

### Discord

Give the hub a Discord application (`--public-url`, `--site-origin`, `--discord-client-id`, and the
secret in `DERETH_HUB_DISCORD_CLIENT_SECRET`) and it turns on its operator API. World operators can
then register, verify and manage their worlds themselves on the site. Players can link characters
too: `/dn link` in game gives a one-time code, which the player enters on the site's
`/characters/` page. A hub without sign-in links no characters.

Give it the application's bot token as well (`--discord-bot-token-file`) and the hub becomes the
Discord broker. No world ever holds a Discord secret. The broker:

- mirrors rooms into Discord channels: the Global room and network channels as the hub's admins
  set them, and a world's own rooms as its operator sets them;
- delivers a tell to a linked character into its player's Discord direct messages while they
  aren't playing it, and carries their reply back;
- lets a player speak in a world's mirror as one of their linked characters, with the bot's
  `/dn say`.

## Linking an Empyrean world

In `empyrean.toml`:

```toml
[federation]
enabled = true
hub_url = "wss://hub.example.org"   # or ws://127.0.0.1:8650 for a hub on the same machine
slug = "coldeve"
key_file = "network.key"            # holds the drth_ key the hub printed
```

With `enabled = true`, a setting the world can't serve (a bad slug or hub address, no key in the
key file, a store it can't read) stops the server at startup with the reason.

The hub, not the world, decides which chat room carries the Global room (Roleplay until the
operator moves it) and which of the world's own rooms are mirrored into Discord. The operator sets
both on the site, or the Global room in game with `/dn admin global <room>`, and the hub tells the
world at once.

To verify the world, an Admin types `/dn verify <token>` with the token the site gives. The hub
sends a small challenge to the world's game port, which must be reachable from the internet as it
is for players. The world posts it back over HTTPS from whatever address its outgoing traffic
uses.

## Checking an implementation

```sh
dereth-network-conformance vectors
dereth-network-conformance hub --url ws://127.0.0.1:8650 --world aa=drth_... --world bb=drth_...
```

[`docs/network/06-conformance.md`](../docs/network/06-conformance.md) lists every check.
