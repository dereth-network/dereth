# The manifest: `plugin.toml`

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A package is a folder (or a zip of one) with `plugin.toml` at its root, its Lua files and its data
files. The manifest names the plugin and says what it needs.

```toml
id = "example.xpmeter"            # lowercase reverse-DNS; unique
name = "XP Meter"                 # shown to the player
version = "1.2.0"                 # SemVer 2
api = "0.1"                       # the lowest API version it needs
entry = "main.lua"                # optional; main.lua when absent
description = "..."               # optional
authors = ["..."]                 # optional
license = "MIT"                   # optional
homepage = "https://..."          # optional

[permissions]                     # optional: requested permissions, each with its reason
"chat.filter" = "Hides the lines you pick."

[capabilities]                    # optional
required = ["ui.window"]          # not loaded without these
optional = ["ui.hud"]             # checked at run time with host.has()

contexts = ["world"]              # optional: where it is active; world, pregame, chargen
replaces = ["example.oldname"]   # optional: ids this plugin takes the place of
ubscript = true                   # optional: a script written for UtilityBelt's scripting

[hosts]                           # optional
exclude = ["dereth-headless"]     # hosts (by host.name) it does not run in

[net]                             # optional, with the net.http permission
domains = ["example.org"]         # the hosts its HTTPS requests may reach, and their subdomains
```

## Rules

- **Unknown keys are refused**, in the top level and in every table. A misspelt permission or
  capability must not be silently dropped. A repeated key is an error of the TOML itself.
- `id`: labels of lowercase letters, digits and `-`, each starting with a letter, separated by
  single dots, at least two labels, at most 128 bytes. An id whose first label is `dereth` is
  kept for the client itself (its plugin panel, its stores and its import folder are kept under
  `dereth.plugins`) and is refused, by the host, the launcher and every tool that reads a
  manifest.
- `version`: `MAJOR.MINOR.PATCH` with no leading zeros, optionally followed by `-pre` and `+build`.
- `api`: `MAJOR.MINOR`. A host runs a plugin when the majors are equal and the host's minor is at
  least the plugin's.
- `entry`: a relative `/`-separated path inside the package, with no empty, `.` or `..` part, and
  the file must exist. A `.lua` or `.luau` entry runs in the Lua engine; a `.wasm` entry is a
  compiled plugin ([wasm.md](wasm.md)). A host with no engine for the entry refuses the package
  with that reason.
- Every permission must be one of [permissions.md](permissions.md), and its value is the reason
  the player is shown when asked.
- Every capability must be one of the host capabilities in the [README](README.md).
- `contexts`: a non-empty list of `world` (a character is in the world), `pregame` (the login
  and character screens, entering the world, after a disconnection) and `chargen` (the
  character creation screens); `["world"]` when absent. A plugin is active only while the
  session is in one of them ([events](events.md#where-a-plugin-runs)). A plugin that logs
  the player in names `pregame`, and `world` too if it does anything once there.
- `net.domains`: host names (dot-separated labels of letters, digits and `-`, at least two, none
  of them a number), each followed by `:port` when its requests go to a port other than 443;
  matched without case. An address is refused. See [peers.md](peers.md#https-net).
- `replaces`: the ids of plugins this one takes the place of, a plugin renamed to a new id. Each
  is an id as above, neither this plugin's own nor one kept for the client. Where the plugin has
  nothing of its own kept yet, a host gives it what the player had under the first of them that
  has it: each of its stores ([api.md](api.md)), opened from the replaced plugin's and kept under
  the new id from then on (the replaced one's is left as it was), and, for a plugin that comes
  with the client, being enabled.

- `ubscript`: `true` for a script written for UtilityBelt's scripting ([ubscript.md](ubscript.md)):
  its entry runs with UtilityBelt's script API as its globals, and is `index.lua` when the
  manifest names none.

**A script's folder.** A folder holding `index.lua` and no `plugin.toml` is a script written for
UtilityBelt's scripting, and a package: its manifest is the host's, the id `ubscript.<the
folder's name>` (lowercase, each run of other characters a `-`), the name the folder's, the entry
`index.lua`, active in the world, with no permission named and no capability needed
([ubscript.md](ubscript.md#installing-a-script)).

## The package

- Files are addressed by `/`-separated paths relative to the package root.
- A package may hold only text and data: `.lua`, `.luau`, `.toml`, `.json`, `.xml`, `.txt`, `.md`,
  `.png`, `.ttf`, `.otf`, and the automation file formats `.utl`, `.nav`, `.met`. No native code
  and no executables.
- At most 2,000 files and 64 MiB.
- `require("lib.util")` loads `lib/util.lua` (or `lib/util.luau`) from the package, once.
