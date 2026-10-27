# {{name}}

A plugin for [Dereth](https://dereth.network).

## Install it

In the Dereth Network launcher, open **Plugins**, paste this address under **Sources** and press
**Add Source**:

```text
{{source_url}}
```

Then install **{{name}}** from the list, and turn it on in the client's plugin panel (`/plugin`).

## Work on it

1. In the Dereth Network launcher, open **Plugins**. Under **Installed**, press **Add Plugin Folder** and choose this folder. (If you used the launcher to create this plugin then this part is automatically done for you.)
2. Play, and turn **{{name}}** on in the client's plugin panel (`/plugin`).
3. Edit, save, and reload with `/plugin reload {{id}}` to check your work.

The plugin interface is [documented here]({{dereth_repo}}/tree/main/docs/plugins).

## Release it

You can release plugins however you like, so long as the plugins.json is correctly written, however, this template is designed around a common GitHub-driven release pattern, assisted by `dereth-plugin`, which comes with Dereth.

Track your changelog under `{{folder}}/CHANGES.md`; anything under `## Unreleased` is promoted automatically on the next release.

```text
# verify the plugin will load correctly in-game
dereth-plugin check {{folder}}

# release the plugin at <version>
dereth-plugin release {{folder}} <version> --push
```

`release` updates the version in `plugin.toml`, commits, tags the
release and pushes the tag to GitHub. The GitHub workflows here will pack the plugin, make a release with the zip, and update the listing in `plugins.json`.

Once, after your first release, you must navigate to your repository **Settings → Pages** and publish from the `gh-pages` branch.

## One folder/repository, many plugins

A single repository can hold several plugins, each a folder beside `{{folder}}`, with a `plugin.toml` and `CHANGES.md`. To add one, run `dereth-plugin new . --name "<name>"` in this directory, or choose this directory in the launcher.

To remove one, delete its folder and push: the workflows drop it from `plugins.json`, and its GitHub releases stay.
