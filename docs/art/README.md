# Making an art pack

An **art pack** replaces the game's visuals with new ones: a remodeled tree, retextured house walls, a new grass texture wherever the original game uses grass. Packs are purely cosmetic. Collision, physics and everything the server sees stay exactly the same, so a pack never affects gameplay, and players with different packs still share the same world.

Packs target things **by their IDs** in the game data: object IDs, texture IDs and terrain types. That's the only connection a pack has to the world, so it works with any data set that uses those IDs, including both the end-of-retail data and the 1999 data. You can even build a pack against the 1999 data to make a remaster of 1999.

This page covers what goes into a pack and how to build one. [sources.md](sources.md) covers how packs are listed, installed and updated, and how to publish your own.

## Pack layout

A pack is a folder with a `manifest.toml` at its root, alongside the glTF 2.0 models and PNG textures it references:

```text
holtburg-houses/
  manifest.toml
  cottage/lod0.gltf
  cottage/lod0.bin
  cottage/walls.png
  ground/grass_albedo.png
  ...
```

All paths in the manifest are relative to the pack folder and use forward slashes. Paths that escape the folder (`..` or an absolute path) are rejected. Because the manifest doesn't depend on where the folder lives, the whole pack can be **packed** into a single file. Each file becomes a record keyed by its path, and textures are stored already processed into the client's own format, so a packed file loads without any decoding or compression. Work in the folder; share the packed file.

The client can load either form, using any of:

- `--art-pack <folder or file>` on the command line
- the `DERETH_ART_PACK` environment variable

The launcher starts the client with whichever packs the player has enabled in the Library. Even then, a pack does nothing until the player turns on the remaster's art pack option and the toggles for the categories it covers (trees, plants, rocks, water plants, building materials, ground).

## The manifest

```toml
[pack]
name = "Holtburg houses"
format = 1
id = "com.example.holtburg-houses"
version = "1.0.0"
description = "Holtburg's cottages and their roofs, remade."
authors = ["Someone"]
license = "CC-BY-4.0"
homepage = "https://example.com/holtburg-houses"
made_for = ["end-of-retail", "1999"]
```

| key | | description |
|---|---|---|
| `name` | | Display name shown in reports and the launcher. |
| `format` | required | Manifest format version. Must be `1`; anything else is rejected. |
| `id` | for listing | The unique ID sources list the pack under, in reverse-domain style: your domain reversed plus a name, as dot-separated segments of lowercase letters, digits and `-`. |
| `version` | for listing | `MAJOR.MINOR.PATCH`. Sources only update a pack to a higher version. |
| `description` | | A sentence or two for players. |
| `authors` | | Who made the pack: one name or a list. |
| `license` | | The license for the pack as a whole. Each model and material also states its own. |
| `homepage` | | Where to find more information. |
| `made_for` | | Which data set the IDs come from: `end-of-retail`, `1999`, or both. |

Only `format` is needed to load a pack locally. The listing keys matter only for sources ([sources.md](sources.md)), and a source can fill in any a pack is missing, such as one packed before those keys existed.

The rest of the manifest lists what the pack replaces: **models** (`[[model]]`), **ground materials** (`[[terrain]]` and `[[steep]]`), **retail texture replacements** (`[[material]]`), and **lighting** preferences (`[lighting]`). If the manifest contradicts itself (a duplicate ID, a terrain type or texture claimed by two materials, LODs out of order, a missing credit), the whole thing is rejected with an error explaining why.

## Models

```toml
[[model]]
id = "0x02000258"          # a setup (0x02...) or a graphics object (0x01...)
foliage = true             # cut-out materials are foliage: two-sided, lit from behind
category = "tree"          # "tree", "plant", "rock" or "water"
author = "Someone"
tool = "Blender 4.2"
license = "CC-BY-4.0"

[[model.lod]]
file = "tree/lod0.gltf"
distance = 40.0            # drawn up to 40 m from the camera

[[model.lod]]
file = "tree/lod1.gltf"    # the last LOD draws at any distance unless it sets one

[[model.lod]]
file = "tree/far.gltf"     # an impostor: pre-rendered views of the model (see "Impostors")
views = 16
columns = 8
```

### Which object a model replaces

`id` is the object the world places. A setup (`0x02...`) is replaced entirely, every part of it, everywhere it appears. A graphics object (`0x01...`) is replaced wherever it's placed on its own, as much of the scenery is. The model uses the placement's position, rotation and scale, so build it in the object's local space: meters, with the same origin and the same up axis as the original. Use `dereth-art survey` ([below](#tools)) to find object IDs and see what they look like.

A model replaces the whole object, so retail texture replacements (see below) don't apply to any object that has a model.

### glTF

Each LOD is a glTF 2.0 file (a `.gltf` with its `.bin` and textures next to it). The client reads:

- triangle meshes with positions, normals, UVs, and optionally tangents and vertex colors
- metallic-roughness materials with a base color texture and factor, a normal map and its scale, a metallic-roughness texture, and an occlusion texture

Textures must be PNG.

### Levels of detail

A model has one or more LODs, listed closest first. Every LOD except the last sets a `distance`, the number of meters from the camera it's drawn out to, and these distances must increase. The last LOD draws at any distance unless it sets one.

A LOD with `shadows_only = true` is rendered only into the sun's shadow maps, for cases where shadows can use a coarser mesh than the view. It's never visible: the view skips straight from the LOD before it to the one after it. Its `distance` is optional. The first and last LODs are always drawn in the view.

### Impostors

A LOD that sets `views` is an impostor: pre-rendered images of the model instead of geometry, for use far away. Its glTF holds a single upright quad across the model's axis (the model's first axis running across, its height running up), which rotates around the model's vertical axis to face the camera.

- **Base color texture:** an atlas of `views` images, `columns` per row (a single row if `columns` is omitted), ordered left to right, then top to bottom. View `k` shows the model from eye level at 360 × k / `views` degrees around its axis, turning from its first axis toward its second, looking at the axis.
- **Normal texture:** for each view, the RGB holds the model's own normal (each component mapped from -1..1 to 0..1), and the alpha holds the surface's depth along the view direction relative to the plane through the axis: 0.5 on the plane, with 0 and 1 being half the quad's width nearer and farther.

### Alpha cutout and two-sided materials

A material using glTF's `MASK` alpha mode is cut out: texels below the cutoff aren't drawn. Before mipmaps are generated, every discarded texel takes the color of the nearest kept texel, so filtering doesn't bleed stray colors into the edges. You can leave the cut-away areas any color you like.

A material marked `doubleSided` in glTF is drawn from both sides. A model with `foliage = true` is treated as foliage: its cut-out materials are drawn two-sided with light passing through them, like leaves.

### Categories

`category` decides which of the player's toggles controls the model: `tree`, `plant`, `rock` or `water` (plants in or near water, like lily pads and reeds). If a model doesn't set one, it counts as a tree if it has an impostor LOD, a plant if it's foliage, and a rock otherwise. A model is only ever `water` if it says so.

### Credits

Every model must set its `author`, the `tool` it was made with, and its `license`. A model missing any of the three is rejected. Be honest about tools, including generative ones.

## Ground

A pack can retexture the terrain: one material per retail terrain type, drawn in place of that type's ground texture wherever retail uses it, blended with retail's own blend masks. Terrain types without a material keep the retail texture.

```toml
[[terrain]]
ground = [1, 9]            # the retail terrain types (0–31) it applies to, or "road"
base_color = "ground/grass_albedo.png"
normal = "ground/grass_normal.png"         # tangent space: red = east, green = north
height = "ground/grass_height.png"         # red channel; where layers meet, the higher one shows
roughness = "ground/grass_roughness.png"   # red channel
occlusion = "ground/grass_occlusion.png"   # red channel
size = 4.0                 # meters covered by one tile of the textures
author = "Someone"
tool = "Substance Designer"
license = "CC0-1.0"
```

Only `base_color`, `size` and the credits are required. Ground textures must be square, tile seamlessly, and be viewed straight from above with north up.

A `[[steep]]` material takes the same keys and is applied to steep slopes of the terrain types it lists, like an obsidian cliff or a sandstone face. Without one, the remaster puts its own scree and rock on steep ground. Each terrain type can have one flat material and one steep material.

## Retail textures

A pack can replace any retail texture wherever a retail object or room uses it: buildings, statics, interiors, dungeon pieces and server-spawned objects alike.

```toml
[[material]]
texture = "0x0500287C"     # the retail texture ID, or a list of them
base_color = "materials/0x0500287C_albedo.png"   # keep the retail texture's alpha
normal = "materials/0x0500287C_normal.png"       # tangent space: red along the texture's
                                                 # width, green up it
roughness = "materials/0x0500287C_roughness.png" # red channel
occlusion = "materials/0x0500287C_occlusion.png" # red channel
normal_scale = 1.0         # how strongly the normal map affects the surface
author = "Someone"
tool = "Krita"
license = "CC-BY-4.0"
```

Only `texture`, `base_color` and the credits are required. Each material lists the retail texture records (`0x05...`) it replaces. The retail surface's UVs, flags (cutout, translucent, additive, luminous) and collision all stay as they are, so your texture has to follow the original's layout: models' UVs map onto it exactly as they did onto the retail texture. Paint over the retail texture, keep its alpha, and everything lines up.

## Lighting

A pack can suggest how its art should be lit, using the same keys and tables as a player's look file, with a separate table for each type of weather:

```toml
[lighting]
sun = 1.1                  # a number, or [[time of day, value], ...] across the day
tint = 0.8

[lighting.rainy]
fog_density = 1.5
```

The available keys:

- `sun`: sun strength, as a multiplier
- `sky`: strength of both ambient lights
- `tint`: the evening and night tint (1 keeps retail's, 0 makes it gray)
- `fog_density` and `fog_color`
- `night_floor`: the minimum ambient level
- `exposure`: in stops, for HDR
- `foliage_hue` and `foliage_pull`: the hue (in degrees) the pack's foliage is pulled toward, and how strongly (0 to 1)

Any key can be overridden for a specific weather type: `sunny`, `rainy`, `clear` or `cloudy`.

The pack's lighting is applied on top of retail's, value by value. A world's own lighting and the player's look file are applied on top of the pack's, so any value they set wins.

## Stacking multiple packs

Players can enable several packs in a set order. The client receives them lowest priority first (one `--art-pack` per pack), while the launcher's Library shows the highest priority at the top. **Later packs win**, key by key:

- models, by object ID
- ground materials, by terrain type (and road), with flat and steep handled separately
- texture replacements, by retail texture
- lighting, by key, including each weather table

Everything else from earlier packs still applies, and anything no pack covers draws as normal. So a pack with just Holtburg's houses, or a single kind of bush, can sit on top of a whole-world pack. The client only loads the content that wins, so a small pack on top of a large one costs no more than the small pack itself.

A packed file can be at most 2 GiB. Split anything larger into several packs and stack them.

## Tools

`dereth-art` lives in the repository's `tools/art`. Build it with `cargo build --release -p dereth-art`.

```text
dereth-art pack <folder> [--out <file>] [--check]
```

Packs the folder into a single file, written to `--out` or to `<folder>.dat` next to the folder. It reads the folder exactly the way the client does, so anything it can't read or process is reported and left out, just as the client would skip it. With `--check`, it writes nothing and instead reports whether the existing file matches what the folder would pack into now, and which records differ.

```text
dereth-art survey [--dat-dir <dir>] [--out <dir>] [--landblocks <XXYY,...>] [--no-thumbnails]
```

Scans the models the world is built from in the game data at `--dat-dir`, and writes a page to `--out` showing where each one is used, how often, what it's made of, and a thumbnail. This is how you find the IDs of objects you want to remake.

To test a pack, start the client with it: use `--art-pack <folder>` while you're working (the folder is re-read on every launch and its textures processed as it loads), and `--art-pack <file.dat>` once it's packed. The client log shows what the pack contained and anything it couldn't read.

## Licensing and credits

A pack is its authors' work, and the client shows who made what. Give every model and material its own `author`, `tool` and `license`, and give the pack its `authors` and `license`. Choose a license that lets players download and use the pack (CC0, CC-BY, CC-BY-SA, or your own terms posted at `homepage`). Only include work you made or have the right to share: a pack contains new art, never the game's own textures or models.
