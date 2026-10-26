# Installing and updating plugins

**Provenance: design.** A walk-through of how each Dereth host finds plugins; the rules for
packages and a release's hash are in [packages.md](../packages.md).

A plugin arrives as a **package**: a folder with `plugin.toml` at its root, or a zip of one. Every
host loads packages the same way once it has them ([lifecycle](lifecycle-and-sandbox.md#loading));
what differs is where each looks. There is no plugin index: a player brings a package, or the URL
of a release's zip.

**Plugins are experimental, and none loads until you turn it on.** In the desktop and browser
clients, tick **Plugins (experimental)** on the Client Options page first: until then no plugin
loads and nothing of the plugins shows. Then every plugin the client finds is listed in the plugin
panel (`/plugin`) after the plugins that are loaded, with the state `off`; choose one and press
**Turn on**, or type `/plugin on <id>` (`/plugin list` names each id). A plugin turned on loads at
once, as any plugin does, and the client remembers it: it loads at every start until you turn it
off (**Turn off**, or `/plugin off <id>`). Its windows start closed: open them from the plugin bar.

| host | where it finds plugins | updates |
|---|---|---|
| the launcher, for the desktop client | installed from a release zip's URL, checked and staged | **Check for updates** fetches the same URL again |
| the desktop client | its settings folder's `plugins` folder, the launcher's installs, and the plugins beside the program; each `--plugins` folder loads whatever the setting says | replace the folder or zip; `/plugin reload <id>`, or `--plugin-dev` |
| the browser client | the folders and zips the player added on the start page, and the plugins beside the page | add it again |
| the headless client | each `--plugins` folder or zip, and `plugin load` in a script, loaded as named | the next run reads the files afresh |

## The plugins that come with the client

Releases carry no plugins while plugins are experimental. The [sample plugins](samples/README.md)
are in the repository, in `dereth/plugins/`: the trackers, the hunting macro and the navigator in
the manner of the add-ons players used, and the smaller samples. A client built from the
repository finds them there, each off until you turn it on. A plugin of the same id in your own
`plugins` folder is yours, and is listed instead.

## The launcher: from a release URL

The launcher's **Plugins** screen installs a plugin for the Dereth client from the URL of its
release's zip.

1. Paste the zip's URL (`https://…/plugin.zip`) and press **Install**. The zip's SHA-256 is read
   from beside it, `<url>.sha256` (a `sha256sum` line, or the hash alone), or pasted in the second
   box. A zip whose hash differs, or that has no hash either way, is not kept.
2. The launcher checks it: it must be a package whose manifest reads.
3. The release is **staged**: kept beside what is installed, not loaded. The screen lists it with
   the permissions its manifest names, which say what it uses. **Accept** installs it; **Discard**
   drops it. Installing never turns a plugin on: the client lists it off until you turn it on
   there.
4. **Check for updates** fetches every plugin's URL again. A release whose zip is the one installed
   or already staged changes nothing. Any other must be the same plugin id as the installed
   release.

**Remove** deletes a plugin. The launcher offers the Dereth client each installed plugin's zip
with `--offer-plugins`; the retail client gets none. On disk the plugins are under the
launcher's data folder (on Windows `%LOCALAPPDATA%\Dereth\launcher`), in `plugins`:
`plugins.json`, and a folder per plugin with `current.zip` and `staged.zip`.

## The desktop client: its plugins folder

The client finds every package in the `plugins` folder of its settings folder (folders holding
`plugin.toml`, and `.zip` files), creating the folder if it is missing, and lists each off until
you turn it on:

| system | the plugins folder |
|---|---|
| Windows | `%APPDATA%\Dereth\client\plugins` |
| macOS | `~/Library/Application Support/Dereth/client/plugins` |
| Linux | `$XDG_CONFIG_HOME/dereth/client/plugins` (`~/.config/dereth/client/plugins`) |

To install a plugin by hand, put its folder or zip there and start the client, then turn it on.
To update it, replace the files and `/plugin reload <id>`. With no plugin at all, the plugin panel
(`/plugin`) and `/plugin list` say where this folder is.

| switch | does |
|---|---|
| `--plugins <folder>` | loads the package in the folder (or zip), or every package in it, whatever **Plugins (experimental)** says; repeatable |
| `--offer-plugins <folder>` | lists the package in the folder (or zip), or every package in it, beside the plugins folder's, each off until turned on; repeatable (the launcher passes its installs) |
| `--no-plugins` | starts with no plugins at all, as the retail client runs |
| `--plugin-dev` | reloads a plugin whenever one of its files changes (checked once a second) |
| `--plugin-policy <file or https URL>` | the world's plugin policy document ([policy.md](../policy.md)); the launcher passes the world's own |

Stores are kept beside the plugins folder in `plugin-store`, one folder per plugin id and one JSON
file per store scope; which plugins are turned on, and where their windows stand, are kept there
too, under `dereth.plugins`.

## The browser client

The start page's **Add a plugin folder…** copies a plugin's folder (the one holding
`plugin.toml`) into the browser's private storage for the site, and **Add a plugin zip…** keeps a
zip there as it is; **Remove all plugins** deletes them. Added plugins are listed in the client,
each off until you turn it on. Their stores live in the same private storage.
Nothing about plugins leaves the player's browser.

## The headless client

`--plugins <folder or zip>` loads a package, or every package one level inside a folder;
repeatable. `plugin load <folder or zip>` in a script loads one more.

| switch | does |
|---|---|
| `--plugin-store <folder>` | keeps stores in the folder; in memory when absent |
| `--plugin-policy <file>` | the world's plugin policy document |
| `--peers <folder>` | the plugins' bus between clients reaches the other headless clients naming the same folder (loopback datagrams; nothing leaves the machine) |

## Publishing a plugin

The author's tool, `dereth-plugin` (`cargo run -p dereth-plugin -- …` from the repository's root),
packs and hashes:

```text
dereth-plugin pack my-plugin my-plugin.zip the folder as a zip; prints the zip's SHA-256
dereth-plugin hash my-plugin.zip           the zip's id, version and SHA-256
```

To publish a release for the launcher, put the zip at a stable URL and its SHA-256 beside it as
`<url>.sha256`; a "latest" URL that always names the newest release makes **Check for updates**
find each new one.
