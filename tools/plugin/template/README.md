# {{name}}

A plugin for [Dereth](https://dereth.network), the Asheron's Call client.

## Install it

In the Dereth launcher, open **Plugins**, paste this address under **Sources** and press **Add
source**:

```text
{{source_url}}
```

Then install **{{name}}** from the list, and turn it on in the client's plugin panel (`/plugin`).

## Work on it

The plugin is the `{{folder}}` folder. To try a change, start the client with it, reloaded as you
save:

```text
dereth-client --plugins {{folder}} --plugin-dev
```

Or copy the folder into the client's plugins folder (`%APPDATA%\Dereth\client\plugins` on
Windows) and turn it on in the client.

Check it as the clients will, with the author's tool of Dereth {{dereth}}
(`dereth-plugin-{{dereth}}-<platform>`, from [that release]({{dereth_repo}}/releases/tag/dereth-v{{dereth}})):

```text
dereth-plugin check {{folder}}
```

The plugin interface is documented in Dereth's repository, under `docs/plugins`.

## Release it

Write what changed under `## Unreleased` in `{{folder}}/CHANGES.md`, then:

```text
dereth-plugin release {{folder}} <version> --push
```

That sets the version in `plugin.toml`, moves the notes under the new version, commits, tags the
release and pushes. The **Release** workflow then packs the plugin, makes a GitHub release with the
zip, and lists it in `plugins.json`, which the `gh-pages` branch publishes on the repository's
GitHub Pages. Launchers that added the source update from there.

Once, after the first release: in the repository's **Settings → Pages**, publish from the
`gh-pages` branch.

## More plugins in this repository

Each plugin is a folder of its own beside `{{folder}}`, with its own `plugin.toml` and
`CHANGES.md`. With more than one, each release is tagged `<id>-v<version>` (`dereth-plugin
release` makes the tag), and all of them are listed in the same `plugins.json`.
