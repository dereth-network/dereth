# Other clients, and the network

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

## Other clients on the machine: `peers`

A player running several clients on one machine can have a plugin coordinate them
(`peers.local` capability and permission):

```lua
peers.send("where", { id = game.player.id, position = game.player.position })
on("peers.message", function(e) print(e.from, e.channel, e.data.id) end)
```

A message reaches **the same plugin** (by id) in every other client on the machine, never the
sender, with `from`, the sending client's character (empty before one is in the world). Its data
is any plain value, at most 64 KiB as JSON; a plugin sends at most 20 a second. A world's policy
may cap how many other clients a plugin hears from (`peers_local`): messages from clients beyond
the cap are dropped, and a cap of 0 makes `peers.send` an error.

The transport is the host's and never leaves the machine: the desktop client uses loopback
datagrams, each client listing its port in the settings folder; the browser client a broadcast
channel between the site's tabs; the headless client a bus inside its process, or with
`--peers <dir>` loopback datagrams between the headless clients that name the same folder.

## HTTPS: `net`

A plugin may fetch from the domains its manifest lists (`net.http` capability and permission):

```toml
[net]
domains = ["example.org", "api.example.net:8443"]   # and their subdomains; port 443 unless named
```

```lua
local r = await(net.fetch("https://maps.example.org/towns.txt"))  -- { status, body, type }
local img = await(net.fetch{ url = "https://maps.example.org/town.png", as = "image" })
ui.get("main.map").src = img                                      -- see ui.md, image
```

`net.fetch{ url, method, headers, body, as }`: `method` is `GET` (the default), `POST`, `PUT` or
`DELETE`; `as = "image"` answers a name an image node shows instead of the body, and fails with
`the answer is not a PNG image` for anything else, and for a picture over 1,024 pixels a side or
one that would take the plugin's pictures over 32 MiB decoded ([ui.md](ui.md)). Only `https://` URLs to a listed host, at the
port listed with it (443 when it names none), are allowed; a URL naming credentials or an address
rather than a host name is refused, and so is a manifest listing an address or a label that is a
number (`127.0.0.1`, `10.0.0.1`, `0x7f.1`, `123.example.org`). A host name that resolves only to
addresses of the player's own machine or a private network (loopback, private, shared and
link-local ranges) is not reached: the desktop client connects only to the public addresses a
name resolves to, and makes the connection itself, through no proxy. (A browser's own fetch
resolves names itself; there the browser's protections apply.) At most four requests at a time
per plugin, bodies at most 8 MiB, no redirects. The
answer settles the promise at a later frame; a failure's reason is the transfer's.

The desktop client fetches off the frame's thread; the browser client uses the browser's fetch (the
site's cross-origin rules apply as well); the headless client has no network for plugins.
