# Installing and updating plugins

**Provenance: design.** A walk-through of how each Dereth host finds plugins; the rules for
packages are in [packages.md](../packages.md), and plugin sources' lists in
[sources.md](../sources.md).

A plugin arrives as a **package**: a folder with `plugin.toml` at its root, or a zip of one. Every
host loads packages the same way once it has them ([lifecycle](lifecycle-and-sandbox.md#loading));
what differs is where each looks. Players get plugins from **plugin sources**: lists of plugins at
their latest release, each one `plugins.json` at an `https://` address. The launcher and the
browser client start with Dereth's own source and nothing installed; a player adds others by their
address.

**Plugins are experimental, and none loads until you turn it on.** In the desktop and browser
clients, tick **Plugins (experimental)** on the Client Options page first: until then no plugin
loads and nothing of the plugins shows. Then every plugin the client finds is listed in the plugin
panel (`/plugin`) after the plugins that are loaded, with the state `off`; choose one and press
**Turn on**, or type `/plugin on <id>` (`/plugin list` names each id). A plugin turned on loads at
once, as any plugin does, and the client remembers it: it loads at every start until you turn it
off (**Turn off**, or `/plugin off <id>`). Its windows start closed: open them from the plugin bar.

| host | where it finds plugins | updates |
|---|---|---|
| the launcher, for the desktop client | installed one by one from plugin sources: Dereth's own, and the ones you add | from each plugin's own source: when you press **Update**, or on their own as the update setting says |
| the desktop client | its settings folder's `plugins` folder, the launcher's installs, and the plugins beside the program; each `--plugins` folder loads whatever the setting says | replace the folder or zip; `/plugin reload <id>`, or `--plugin-dev` |
| the browser client | the folders and zips the player added on the start page, and the plugins installed there from plugin sources: Dereth's own, and the ones you add | as the launcher does, on the start page; a folder or zip you added: add it again |
| the headless client | each `--plugins` folder or zip, and `plugin load` in a script, loaded as named | the next run reads the files afresh |

## Dereth's plugins

The [sample plugins](samples/README.md) are in the repository, in `dereth/plugins/`: the trackers,
the hunting macro and the navigator in the manner of the add-ons players used, and the smaller
samples. They reach players through **Dereth's own source**, which lists each of them and which
the launcher and the browser client start with: install the ones you want, as from any source. No release carries them
installed. While plugins are experimental the source is not published yet.

A client built from the repository finds the samples in `dereth/plugins/` itself, each off until
you turn it on. A plugin of the same id in your own `plugins` folder is yours, and is listed
instead.

## The launcher: plugin sources

The launcher has a **Plugins** page when it is started with `--plugins`. It installs plugins for
the Dereth client from plugin sources, and keeps them updated.

The page has three sections, **Installed**, **Available** and **Sources**; a section's heading
folds it away and brings it back. A plugin's row shows its name, version and source and the first
line of what it does; click it and it opens to everything known about it: the whole description,
its id, authors and homepage, what it uses (its permissions and the web domains it reaches), when it
was released, what changed in it and its size. One row is open at a time.

**Sources.** Each source is a line: its name, how many plugins it lists and its address, or why it
could not be read. Dereth's own source is always there. To add another, paste the address of its
`plugins.json` under **Sources** and press **Add source**; a line under it says that Dereth does not
review plugins from other sources, and **Add** adds it. **Remove** takes a source away; the plugins
installed from it stay, with no more updates. The launcher reads every source each time it starts,
and again when you press **Refresh**, which says what is new: plugins a source lists now, updates
applied and waiting, and sources that could not be read.

**Available.** The plugins the sources list that you have not installed, each saying which source
lists it. **Search** narrows the list. Press **Install** on the one you want. The launcher downloads
its zip and keeps it only when it is the file the source lists and its manifest says what the
source says ([the checks](../sources.md#checking-a-release)). A release that needs a newer plugin
API than your client says **Needs a newer Dereth** and cannot be installed. An id under
`network.dereth.` installs only from Dereth's own source. Installing never turns a plugin on: the
client lists it off until you turn it on there.

**Installed.** Each installed plugin updates only from the source it came from, only to a newer
release. When an update waits, its row says so, with what the release asks for that the installed
one did not; **Update** installs it. **Remove** deletes the plugin.

**Automatically update plugins** decides which plugins update on their own when the launcher finds
new releases:

| setting | what happens |
|---|---|
| None | nothing updates on its own; the launcher says that updates are waiting, and you update each one |
| All | every plugin updates on its own |
| Only Dereth | the plugins from Dereth's own source update on their own; the others wait (the setting at first) |

Whatever the setting, a release that asks for a permission or a web domain the installed one did
not waits for you, and one that needs a newer Dereth is never installed.

The launcher offers the Dereth client each installed plugin's zip with `--offer-plugins`; the
retail client gets none. On disk the plugins are under the launcher's data folder (on Windows
`%LOCALAPPDATA%\Dereth\launcher`), in `plugins`: `sources.json` (the sources, the last list each
gave and the settings), `installed.json`, and a folder per plugin with its `current.zip`.

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
zip there as it is; **Remove all plugins** deletes them, and the plugins installed from sources.
Kept plugins are listed in the client, each off until you turn it on. Their stores live in the same
private storage.

**Plugin sources.** Under *Plugin sources* the start page does what the launcher's Plugins page
does, with the launcher's own code. Dereth's own source is always there. To add another, paste the
address of its `plugins.json` and press **Add source**, then **Add** on the line that says Dereth
does not review plugins from other sources. **Remove** takes a source away; the plugins installed
from it stay, with no more updates. Each source is listed with the plugins installed from it and
then the ones it offers that are not installed, each saying what it does and what it uses, with
**Install**, **Update** or **Remove**. A zip is kept only when it passes the launcher's checks
([sources.md](../sources.md#checking-a-release)), an id under `network.dereth.` installs only from
Dereth's own source, and an installed plugin updates only from its own source, to a newer release.
The page reads every source each time it opens, and again when you press **Refresh**; its
**Automatically update plugins** setting is the launcher's, and an update that asks for more
permissions waits for you.
The sources and what was installed from them are kept in the browser's private storage.

**What a page can read.** A web page can read a list or a zip only from a host that lets any page
read it (`Access-Control-Allow-Origin`): Cloudflare Pages with a `_headers` file, GitHub Pages,
GitLab Pages and `raw.githubusercontent.com` do. A page cannot tell a host that does not from one
that is offline, so such a source says "Couldn't read this source here. Its host may not let web
pages read it.", and the other sources work as usual. A plugin whose zip is a GitHub release
download says **Desktop launcher only**: GitHub does not let web pages download release files, and
the launcher can.

Nothing about plugins leaves the player's browser but the requests for the lists and zips of
Dereth's source and the sources you added, each to its own host.

## The headless client

`--plugins <folder or zip>` loads a package, or every package one level inside a folder;
repeatable. `plugin load <folder or zip>` in a script loads one more.

| switch | does |
|---|---|
| `--plugin-store <folder>` | keeps stores in the folder; in memory when absent |
| `--plugin-policy <file>` | the world's plugin policy document |
| `--peers <folder>` | the plugins' bus between clients reaches the other headless clients naming the same folder (loopback datagrams; nothing leaves the machine) |

## Publishing a plugin

A plugin is published by listing its release in a plugin source. The author's tool,
`dereth-plugin`, makes a plugin repository whose GitHub workflows check every push and, on a
release tag, pack the plugin and list it in the repository's own source:
[publishing.md](publishing.md).
