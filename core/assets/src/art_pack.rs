//! An art pack's manifest: which retail objects the pack has models for, the files of each
//! model's levels of detail and the distances they are drawn to, and who made each model, with
//! what, under which license.
//!
//! A pack is a folder: `manifest.toml` at its top, beside the glTF 2.0 models it names. Every
//! file is named by a path relative to the folder, with `/` between its parts and nothing that
//! climbs out of the folder, so the manifest says nothing about where the folder is. That is what
//! lets a pack be packed into one data file: each file the manifest names becomes a record keyed
//! by its path, and the manifest itself one more, read through the same paths.
//!
//! ```toml
//! [pack]
//! name = "Remastered"
//! format = 1
//! id = "network.dereth.remastered"   # what a source lists it by; see below
//! version = "1.0.0"
//! description = "Trees, plants and rocks made again."
//! authors = ["..."]
//! license = "CC-BY-4.0"
//! homepage = "https://..."
//! made_for = ["end-of-retail", "1999"]
//!
//! [[model]]
//! id = "0x02000258"          # a setup (0x02...) or a graphics object (0x01...)
//! foliage = true             # its cut-out materials are growth: two-sided, lit through
//! category = "tree"          # "tree", "plant", "rock" or "water"; see below
//! author = "..."
//! tool = "..."
//! license = "..."
//!
//! [[model.lod]]
//! file = "tree/lod0.gltf"
//! distance = 40.0            # drawn out to this many metres from the eye
//!
//! [[model.lod]]
//! file = "tree/lod1.gltf"    # the last level is drawn at any distance unless it names one
//!
//! [[model.lod]]
//! file = "tree/far.gltf"     # pictures of the model seen from round it (see `views`)
//! views = 16
//! columns = 8
//! ```
//!
//! A level that names `shadows_only = true` is drawn into the sun's shadows alone, where they ask
//! for a coarser level than the view draws, and never into the view, which goes from the level
//! before it straight to the level after it; its distance, which it may leave out, is not read.
//! The first and the last level are always drawn into the view.
//!
//! A level that names `views` is pictures of the model, not its shape: its glTF holds an upright
//! quad across the model's axis (its first axis across, its height up), which is turned to face
//! the eye about the model's upright axis. Its base colour picture holds `views` pictures of the
//! model laid in tiles, `columns` to a row (all in one row when it names none), left to right
//! and then down; view `k` is the model seen level from 360 k / `views` degrees round its axis,
//! from its first axis toward its second, looking at the axis. Its normal picture holds, for each
//! view, the model's own normal in its colour (each part -1..1 as 0..1) and, in its alpha, how
//! far the surface stands along the view from the plane through the axis (0.5 on the plane, 0
//! and 1 half the quad's width nearer and farther).
//!
//! **A tree's levels far off.** A model of `category = "tree"` walks down its levels by their
//! distances, each taking over no nearer than the distance its level names (farther on a large
//! screen, where a coarse level would still show), fading into the next. Past its last level
//! drawn into the view it is drawn as pictures of its nearest level, which the client makes, and
//! is wholly those pictures before the edge of the blocks round the viewer: a tree of one level
//! does so too. Past the blocks round the viewer, where the client grows the
//! world's trees as pictures alone, it is drawn from its level that names `views` when it has
//! one, and otherwise from pictures the client makes of its nearest level: a pack need not
//! draw its own.
//!
//! A pack may also lay the ground: a material for each retail terrain type it covers, drawn in
//! place of that type's ground picture wherever retail lays it, through retail's own blend
//! masks. A type the pack names no material for keeps retail's picture.
//!
//! ```toml
//! [[terrain]]
//! ground = [1, 9]            # the retail terrain types (0 to 31) it is drawn for, or "road"
//! base_color = "ground/grass_albedo.png"
//! normal = "ground/grass_normal.png"         # tangent space: red east, green north
//! height = "ground/grass_height.png"         # red; where layers meet, the higher one shows
//! roughness = "ground/grass_roughness.png"   # red
//! occlusion = "ground/grass_occlusion.png"   # red
//! size = 4.0                 # metres one repeat of the pictures covers
//! detail = "ground/grain.png"                # a fine grain over the ground close by
//! detail_size = 1.0          # meters one repeat of the detail covers
//! detail_strength = 1.0      # how strongly the detail changes the ground: 0 not at all
//! pebbles = true             # whether the presentation's own pebbles lie on these grounds
//! author = "..."
//! tool = "..."
//! license = "..."
//! ```
//!
//! Only `base_color`, `size` and the credit are required. The pictures of every material are
//! square and repeat seamlessly, seen from straight above with north up.
//!
//! The detail picture is a fine grain laid over the material close by, repeating every
//! `detail_size` meters (1 unless named) and fading out over the first few tens of meters. It is
//! multiplied in at twice its value, in display values: mid-gray (128) leaves the ground as it is,
//! lighter brightens it and darker darkens it, so a gray picture of mean 128 keeps the ground's
//! tone. It is part of the pack's look: drawn wherever the pack's ground is drawn, whatever the
//! game's own detail-texture setting says (that setting is for the game's own ground), and gone
//! with the rest of the pack's ground when that is turned off. The Asheron's Call 2 look fills it
//! with AC2's own ground grain, which AC2 itself loaded but never drew. `detail_strength` scales
//! how far the detail moves the ground from its own color: 1 (unless named) as the picture is
//! made, 0 not at all, and more than 1 further.
//!
//! The presentation lays small pebbles of its own on bare ground, on slopes and along the
//! road's edges. `pebbles = false` keeps them off the grounds the material names wherever the
//! pack's ground is drawn: their flat ground and their slopes, and for the road, its edges. Only
//! a `[[terrain]]` material names it; the slopes of its grounds follow it whatever `[[steep]]`
//! material lies there.
//!
//! A `[[steep]]` material, with the same keys but `pebbles`, is laid on the steep slopes of the
//! terrain types it names, in place of the scree and rock the presentation lays on steep ground of
//! a type with none: an obsidian cliff, a sandstone face. A type may have one of each kind.
//!
//! A pack may also grow ground cover: small models of its own (flowers, ferns, reeds, litter)
//! scattered over the terrain types each names, near the eye, beside whatever retail places there.
//!
//! ```toml
//! [[cover]]
//! ground = [1, 9]            # the retail terrain types (0 to 31) it grows on
//! density = 4.0              # how many grow on 100 square meters of flat ground of those types
//! scale = [0.8, 1.25]        # each drawn at a size between these, the same on every axis
//! max_slope = 0.6            # none where the ground rises more than this a meter across
//! foliage = true
//! category = "plant"
//! author = "..."
//! tool = "..."
//! license = "..."
//!
//! [[cover.lod]]
//! file = "cover/lupins.gltf"
//! distance = 40.0            # the last level's distance is how far from the eye it grows
//! ```
//!
//! A cover's levels, `foliage`, `category` and credit are a model's, except that its last level
//! must name a distance: past it the cover is not drawn. `scale` defaults to `[1, 1]` and
//! `max_slope` to 0.6. Covers that name the same levels share one model, each growing it on its
//! own types as thickly as it says. Covers of several packs in a stack all grow, unless two name
//! the same first file: then the one above wins. Where a cover grows, a pack can thin or take away the grass and
//! flowers the presentation grows itself through its `[lighting]`'s grass tables. A cover stands
//! for no retail record, so it has no era of its own: a `fingerprint` or `draws_on` on it is read
//! past like any other key it does not use.
//!
//! A pack may also draw a picture of its own in place of a retail picture, wherever a retail
//! object or room is drawn with it: buildings, statics, rooms, dungeon pieces and what the
//! server places alike. Each such material names the retail pictures (texture records, `0x05...`)
//! it stands for. The retail surface's coordinates, its flags (cut out, translucent, additive,
//! luminous) and its collision are kept, so the picture keeps the retail one's layout: a model's
//! coordinates point into it as they point into the retail picture. A model of the pack's own
//! is drawn in place of the whole object, so where the pack has one, its materials do not reach.
//!
//! ```toml
//! [[material]]
//! texture = "0x0500287C"     # the retail picture, or a list of them
//! base_color = "materials/0x0500287C_albedo.png"   # with the retail picture's alpha
//! normal = "materials/0x0500287C_normal.png"       # tangent space: red along the picture's
//!                                                  # width, green up it
//! roughness = "materials/0x0500287C_roughness.png" # red
//! occlusion = "materials/0x0500287C_occlusion.png" # red
//! height = "materials/0x0500287C_height.png"       # red; white the surface, black its deepest
//! normal_scale = 1.0         # how strongly the normal picture bends the normal
//! keep_retail_picture = true # draw the player's own retail picture, not base_color
//! damp = 0.8                 # 0 to 1: how damp it looks in a room
//! damp_map = "materials/0x0500287C_damp.png"       # red: white as damp as damp says, black dry
//! author = "..."
//! tool = "..."
//! license = "..."
//! ```
//!
//! Only `texture`, `base_color` and the credit are required. A material that names
//! `keep_retail_picture = true` keeps drawing the retail picture, in whichever form the player's
//! files hold it, and adds only what its other pictures say of the surface: its relief, its
//! roughness and occlusion. Its `base_color` is then a flat colour for a reader that draws no
//! relief, and is neither read nor drawn here. Such a pack carries none of the retail art: the
//! player's own copy is drawn, and the pack's pictures say only how its surface lies.
//!
//! A material's pictures are packed as the base color (none where it keeps the retail picture),
//! the normal at its own size, and one surface picture at about half its pictures' size holding
//! the roughness, the occlusion, the height and how damp it is in red, green, blue and alpha.
//! `metal`, `brighten`, `gloss`, `clear`, `refraction` and `wet` say what it is made of; a
//! material that names one of them needs no picture of its own.
//!
//! **What a reader may leave out.** A reader of this format draws what it knows of a pack and
//! leaves the rest, refusing nothing for it: a key it does not know is read past, and a table it
//! does not know (`[[light]]`, `[[moon]]`) is not read. This reader leaves out
//! `height`, `damp` and `damp_map` once packed (they shape the surface picture, which it reads),
//! and a material's `grit`, `grit_repeat`, `damp_floors`, `slime`, `ice`, `sheen` and what it is
//! made of; a model's `wind`; a ground's colours beyond its pictures. Each is drawn as if the pack
//! had not named it.
//!
//! **A model's category** says which of a player's switches turns it on: `tree`, `plant`, `rock`
//! or `water` (growth in and by the water: lily pads, reeds). A model that names none is a tree
//! when it has a level of pictures round it, a plant when it is foliage, and a rock otherwise; a
//! model is a water model only when it says so.
//!
//! **A pack may draw a body in place of a setup's parts** (`[[skin]]`): one skinned glTF 2.0 mesh,
//! every object built on the setup drawn with it, standing where the object stands and posed only
//! by its own clips. Joints named `part_00`, `part_01` ... (or with `extras.part`) name the parts
//! it is drawn in place of, by their place in the setup: those draw nothing, and any other part
//! draws as it is. Every other joint is a bone of the body's own skeleton. Nothing else about the
//! object changes.
//!
//! ```toml
//! [[skin]]
//! setup = "0x02000001"       # the setup whose objects it is drawn for
//! file = "human/body.gltf"
//! author = "..."
//! tool = "..."
//! license = "..."
//! ```
//!
//! A setup may have several bodies, each for the objects whose look carries one of its
//! `palettes` (palette ids, `0x04...`, as a server's description of the object lays them over
//! its parts): the first that matches is drawn, and a body that names none is drawn for the rest.
//! A body may also name the fingerprint of the setup it was made for, as a model does (below), so
//! a pack can carry one body for each era's setup under the same id.
//!
//! A body's clips (`clips`) name an animation of its glTF, by name, for each motion slot it
//! fills ([`CLIP_SLOTS`]). The slots are the same for every creature: the client plays the one for
//! what the object's motion does, and for a slot the body leaves empty, another's (a side step the
//! walk, a run the walk, a stance's idle the peace idle, an attack the medium one, a sleep the dead
//! pose). Idles, turns, sleep, the dead pose and a fall loop at their own pace; walks, runs and
//! side steps go round once for each stride their object's motion asks; the rest play once, a
//! death holding its end. A turn or a side step is to the right: to the left plays it backwards.
//!
//! ```toml
//! clips = { idle = "Idle", walk = "Walk", run = "Run", melee_med = "Melee_Normal1", die = "Death" }
//! ```
//!
//! A clip's glTF animation may carry `extras.stride`, the meters of the body's own space one round
//! of a walk, run or side step carries it, so its feet hold; and `extras.hit`, the seconds from an
//! attack's start to its blow, which the client lands when the object's own attack does. A table
//! keyed by animation ids (`"0x03000044" = "run"`), as packs once had, still opens.
//!
//! A body draws nothing its object holds: the object still holds it.
//!
//! A pack may keep some objects as they are (`[keep]`), whatever bodies it has for their setups:
//! those of the weenies it lists (by their class ids, as a server names them: a statue built on a
//! creature's setup, a boss), and, with `launchers`, any that has held a bow, crossbow, sling or
//! atlatl since it was made (a body that cannot shoot).
//!
//! ```toml
//! [keep]
//! weenies = [7, 237, 19196]
//! launchers = true
//! ```
//!
//! **A pack may say how its art wants to be lit** (`[lighting]`), with the keys and the tables
//! of a player's look file, each kind of day a table of its own within it:
//!
//! ```toml
//! [lighting]
//! sun = 1.1                  # a number, or [[time of day, value], ...] round the day
//! tint = 0.8
//!
//! [lighting.rainy]
//! fog_density = 1.5
//! ```
//!
//! The manifest keeps the section's text ([`ArtPackManifest::lighting`]); what reads looks reads
//! it.
//!
//! **What a source lists the pack by** ([`About`]): `id`, `version`, `description`, `authors`,
//! `license`, `homepage` and `made_for` in `[pack]`, which a pack needs only to be listed by an
//! art pack source (`dereth-art-catalog`); a pack opened from its folder or its file needs
//! none of them. The id is lowercase words joined by dots, its author's own domain reversed and
//! a name (`com.example.holtburg-houses`); the version is `MAJOR.MINOR.PATCH`; `made_for` names
//! the data the pack's ids are chosen from, `end-of-retail`, `2005` or `1999` (a pack only
//! replaces things by their ids, so it draws on any world whose data has them).
//!
//! **Which era's record an entry is for.** The eras' files hold other art under some of the same
//! ids: Throne of Destiny repainted pictures and remade models in place. A `[[model]]`, `[[skin]]`,
//! `[[material]]`, `[[terrain]]` or `[[steep]]` may name the fingerprint of the retail record it
//! was made from ([`crate::fingerprint`]), and is then drawn only on a world whose record of that
//! id matches it, so one id may have an entry for each era's record of it:
//!
//! ```toml
//! [[material]]
//! texture = "0x0500001F"
//! fingerprint = "picture 64x128 0000f5ffffc30000"   # end of retail's picture
//! ...
//!
//! [[material]]
//! texture = "0x0500001F"
//! fingerprint = "picture 32x64 008cecf7f7fffff7"    # 2005's picture under the same id
//! ...
//! ```
//!
//! A material for several pictures names a list of fingerprints, one for each. A `[[terrain]]`
//! or `[[steep]]` material's fingerprint is that of the picture the world's region lays its
//! grounds with. A `[[skin]]`'s is that of its setup (`model <hash>`, as a `[[model]]` names).
//! An entry that names none is drawn on any record, as before. `draws_on = "any"` draws a model,
//! body or material on any record whatever its fingerprint, and `draws_on = "none"` on
//! none. Where several entries are for one id (for bodies, one setup and its palettes), the first
//! that is drawn on the world's record draws it; two entries for one id may not name the same fingerprint, and an entry that names
//! none may only come after those that do. A material may name `palettes`, the palette records
//! (`0x04...`) it is drawn in alone, for one picture several creatures wear, told apart by
//! palette; a reader that does not tell palettes apart leaves such a material out.
//!
//! This module reads the manifest only. The models and their pictures are read by whatever
//! draws them.

use std::collections::HashMap;

use dereth_primitives::DataId;

pub use crate::fingerprint::Fingerprint;

/// The manifest format this module reads.
pub const FORMAT: i64 = 1;

/// The manifest's file name at the top of a pack's folder.
pub const MANIFEST_FILE: &str = "manifest.toml";

/// A pack's manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtPackManifest {
    /// The pack's name.
    pub name: String,
    /// What a source lists it by, where it says.
    pub about: About,
    /// Its models, in the order the manifest lists them.
    pub models: Vec<ManifestModel>,
    /// Its ground materials, in the order the manifest lists them.
    pub grounds: Vec<ManifestGround>,
    /// Its materials for retail pictures, in the order the manifest lists them.
    pub materials: Vec<ManifestMaterial>,
    /// Its ground cover, in the order the manifest lists it. Each cover's model is also in
    /// [`Self::models`], under [`ManifestCover::id`].
    pub covers: Vec<ManifestCover>,
    /// Its bodies drawn in place of setups' parts, in the order the manifest lists them.
    pub skins: Vec<ManifestSkin>,
    /// The weenies (class ids) whose objects keep their own parts, whatever bodies it has
    /// (`[keep]`), in the order the manifest lists them.
    pub keep: Vec<u32>,
    /// Whether an object that has held a launcher keeps its own parts (`[keep]`'s `launchers`).
    pub keep_launchers: bool,
    /// Its `[lighting]` section, as the text of a look file: its keys at the top and each kind
    /// of day's table under its own name. `None` when it has none.
    pub lighting: Option<String>,
}

/// What an art pack source lists a pack by: the optional keys of its `[pack]` table. Each is
/// empty where the manifest leaves it out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct About {
    /// Lowercase words joined by dots (`com.example.holtburg-houses`).
    pub id: String,
    /// `MAJOR.MINOR.PATCH`.
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    /// The license the pack as a whole is under; each model's own credit says its own.
    pub license: String,
    pub homepage: String,
    /// The data its ids are chosen from: `end-of-retail`, `1999`.
    pub made_for: Vec<String>,
}

/// The data a pack's ids may be chosen from ([`About::made_for`]).
pub const MADE_FOR: [&str; 3] = ["end-of-retail", "2005", "1999"];

/// Whether `id` is a pack id: lowercase words of letters, digits and `-`, joined by dots, at
/// least two of them, at most 128 bytes in all.
#[must_use]
pub fn is_pack_id(id: &str) -> bool {
    let word = |w: &str| {
        !w.is_empty()
            && !w.starts_with('-')
            && !w.ends_with('-')
            && w.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    id.len() <= 128 && id.split('.').count() >= 2 && id.split('.').all(word)
}

/// Whether `version` is `MAJOR.MINOR.PATCH`, three whole numbers with no leading zero.
#[must_use]
pub fn is_pack_version(version: &str) -> bool {
    let mut parts = version.split('.');
    let number = |p: Option<&str>| {
        p.is_some_and(|p| {
            !p.is_empty()
                && p.len() <= 9
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p == "0" || !p.starts_with('0'))
        })
    };
    number(parts.next()) && number(parts.next()) && number(parts.next()) && parts.next().is_none()
}

fn about(pack: &toml::Table) -> Result<About, ManifestError> {
    let text = |key: &str| -> Result<String, ManifestError> {
        match pack.get(key) {
            None => Ok(String::new()),
            Some(toml::Value::String(s)) => Ok(s.trim().to_owned()),
            Some(_) => fail(format!("[pack] {key} is not text")),
        }
    };
    let list = |key: &str| -> Result<Vec<String>, ManifestError> {
        match pack.get(key) {
            None => Ok(Vec::new()),
            Some(toml::Value::String(s)) => Ok(vec![s.trim().to_owned()]),
            Some(toml::Value::Array(a)) => a
                .iter()
                .map(|v| match v {
                    toml::Value::String(s) => Ok(s.trim().to_owned()),
                    _ => fail(format!("[pack] {key} is not a list of text")),
                })
                .collect(),
            Some(_) => fail(format!("[pack] {key} is not a list of text")),
        }
    };
    let about = About {
        id: text("id")?,
        version: text("version")?,
        description: text("description")?,
        authors: list("authors")?,
        license: text("license")?,
        homepage: text("homepage")?,
        made_for: list("made_for")?,
    };
    if !about.id.is_empty() && !is_pack_id(&about.id) {
        return fail(format!(
            "[pack] id {:?} is not lowercase words joined by dots",
            about.id
        ));
    }
    if !about.version.is_empty() && !is_pack_version(&about.version) {
        return fail(format!(
            "[pack] version {:?} is not MAJOR.MINOR.PATCH",
            about.version
        ));
    }
    if let Some(bad) = about
        .made_for
        .iter()
        .find(|m| !MADE_FOR.contains(&m.as_str()))
    {
        return fail(format!(
            "[pack] made_for names {bad:?}, not {}",
            MADE_FOR.join(" or ")
        ));
    }
    Ok(about)
}

/// The first id a cover's model is listed under: `COVER_ID | ` 24 bits of its first file's
/// hash, a range no retail object uses.
pub const COVER_ID: u32 = 0xEC00_0000;

/// The id a cover whose first level is `file` is listed under ([`COVER_ID`]).
#[must_use]
pub fn cover_id(file: &str) -> DataId {
    // FNV-1a: the same file, the same id, on every machine and in every build.
    let mut h: u32 = 0x811C_9DC5;
    for b in file.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    DataId(COVER_ID | (h ^ (h >> 24)) & 0x00FF_FFFF)
}

/// Whether `id` is a cover's ([`COVER_ID`]).
#[must_use]
pub fn is_cover(id: DataId) -> bool {
    id.0 & 0xFF00_0000 == COVER_ID
}

/// One ground cover: the model it grows, the terrain types it grows on and how thickly.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestCover {
    /// Its model's id in [`ArtPackManifest::models`] ([`cover_id`]).
    pub id: DataId,
    /// The terrain types it grows on, 0 to 31.
    pub grounds: Vec<u8>,
    /// How many grow on 100 square meters of flat ground.
    pub density: f32,
    /// The least and the most it is drawn at.
    pub scale: (f32, f32),
    /// The steepest ground it grows on: its rise a meter across.
    pub max_slope: f32,
    /// How far from the eye it grows: its last level's distance.
    pub reach: f32,
}

/// One material drawn in place of retail pictures: the pictures it stands for and its own.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestMaterial {
    /// The retail pictures (texture records, `0x05...`) it is drawn in place of.
    pub textures: Vec<DataId>,
    /// The base color picture, with the retail picture's alpha.
    pub base_color: String,
    /// The tangent-space normal picture: red along the picture's width, green up it.
    pub normal: Option<String>,
    /// The roughness picture, in its red.
    pub roughness: Option<String>,
    /// The ambient occlusion picture, in its red.
    pub occlusion: Option<String>,
    /// The height picture, in its red: white the surface, black its deepest.
    pub height: Option<String>,
    /// How strongly the normal picture bends the normal.
    pub normal_scale: f32,
    /// Draw the player's own retail picture, not [`Self::base_color`], which is then a flat
    /// colour for a reader that draws no relief and is neither read nor drawn here.
    pub keep_retail_picture: bool,
    /// How damp it looks in a room, 0 to 1.
    pub damp: f32,
    /// Where it is damp, in its red: white as damp as [`Self::damp`] says, black dry.
    pub damp_map: Option<String>,
    /// What it is made of beyond its pictures.
    pub matter: Matter,
    /// The fingerprint of the retail picture each of [`Self::textures`] was made from, where it
    /// names one.
    pub fingerprints: Vec<Option<Fingerprint>>,
    /// Which records it is drawn on.
    pub draws_on: DrawsOn,
    /// The palettes (`0x04...`) it is drawn in alone; any, where it names none.
    pub palettes: Vec<DataId>,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

/// The motion slots a body's clips fill ([`ManifestSkin::clips`]), as a pack names them: a
/// stance's idle (peace, melee, missile, magic); walking, walking backwards, running, a side step
/// and a turn; attacks high, medium and low, a special, a missile, a cast and the warning before
/// one; a hit and a heavy hit; dying, dead and asleep; a change of stance; an emote and a cheer;
/// falling and a jump.
pub const CLIP_SLOTS: [&str; 26] = [
    "idle",
    "idle_melee",
    "idle_missile",
    "idle_magic",
    "walk",
    "walk_back",
    "run",
    "side_step",
    "turn",
    "melee_high",
    "melee_med",
    "melee_low",
    "special",
    "missile",
    "cast",
    "telegraph",
    "hit",
    "hit_heavy",
    "die",
    "dead",
    "sleep",
    "stance",
    "emote",
    "cheer",
    "fall",
    "jump",
];

/// One body drawn in place of a setup's parts: the setup and its glTF.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestSkin {
    /// The setup (`0x02...`) whose objects it is drawn for.
    pub setup: DataId,
    /// The glTF 2.0 file, relative to the pack's folder.
    pub file: String,
    /// The palettes (`0x04...`) an object's look must carry one of for this body to be drawn for
    /// it; none for the setup's other objects.
    pub palettes: Vec<DataId>,
    /// The body's own clips: each slot ([`CLIP_SLOTS`]) and its glTF animation's name.
    pub clips: Vec<(String, String)>,
    /// Clips listed by the animation (`0x03...`) they were played for, as packs once named them.
    pub by_animation: Vec<(DataId, String)>,
    /// The fingerprint of the retail setup it was made for, where it names one.
    pub fingerprint: Option<Fingerprint>,
    /// Which records of its setup it is drawn on.
    pub draws_on: DrawsOn,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

/// What a material is made of beyond its pictures: a metal, how smooth, or clear as ice or a gem.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Matter {
    /// A metal, its picture brightened this many times to its colour.
    pub metal: Option<f32>,
    /// How smooth it is where no roughness picture says, 0 to 1.
    pub gloss: Option<f32>,
    /// Clear in its picture's own colour, this share of the world behind seen through it.
    pub clear: Option<f32>,
    /// How strongly a clear material bends light: its index of refraction.
    pub refraction: f32,
    /// How much of a film of water lies on it, 0 to 1.
    pub wet: Option<f32>,
}

impl Matter {
    /// Whether it says nothing beyond its pictures.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.metal.is_none() && self.gloss.is_none() && self.clear.is_none() && self.wet.is_none()
    }
}

/// Which retail records an entry is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawsOn {
    /// Those whose fingerprint matches its own; any, where it names none.
    #[default]
    Matching,
    /// Any record of its id.
    Any,
    /// None: the entry is not drawn.
    Nothing,
}

impl DrawsOn {
    /// Whether an entry made from a record of fingerprint `made_from` is drawn on a record of
    /// fingerprint `record`; a record whose fingerprint is not known is drawn on as any.
    #[must_use]
    pub fn draws(self, made_from: Option<&Fingerprint>, record: Option<&Fingerprint>) -> bool {
        match (self, made_from, record) {
            (Self::Nothing, _, _) => false,
            (Self::Matching, Some(a), Some(b)) => a.matches(b),
            _ => true,
        }
    }
}

/// A retail ground a pack's material is drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ground {
    /// A terrain type, 0 to 31.
    Type(u8),
    /// The road laid over the terrain.
    Road,
}

/// One ground material: the retail grounds it is drawn for, its pictures and how large a repeat
/// of them is.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestGround {
    /// The retail grounds it is drawn for.
    pub grounds: Vec<Ground>,
    /// The base colour picture.
    pub base_color: String,
    /// The tangent-space normal picture, red toward east and green toward north.
    pub normal: Option<String>,
    /// The height picture, in its red: where two grounds meet, the higher shows through.
    pub height: Option<String>,
    /// The roughness picture, in its red.
    pub roughness: Option<String>,
    /// The ambient occlusion picture, in its red.
    pub occlusion: Option<String>,
    /// How many metres one repeat of the pictures covers.
    pub size: f32,
    /// How strongly the normal picture bends the normal.
    pub normal_scale: f32,
    /// The detail picture: a fine grain multiplied in at twice its value close by.
    pub detail: Option<String>,
    /// How many meters one repeat of the detail picture covers.
    pub detail_size: f32,
    /// How strongly the detail picture changes the ground: 1 as made, 0 not at all.
    pub detail_strength: f32,
    /// Whether the presentation's own pebbles lie on its grounds: on their flat, their slopes
    /// and, for the road, along its edges. Only a `[[terrain]]` table names it.
    pub pebbles: bool,
    /// Laid on the steep slopes of its grounds rather than on their flat ground: a `[[steep]]`
    /// table's.
    pub steep: bool,
    /// The fingerprint of the picture the world's region lays its grounds with in the world it
    /// was made for, where it names one.
    pub fingerprint: Option<Fingerprint>,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

/// One model: the retail object it is drawn in place of, its levels of detail, how it is lit
/// and who made it.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestModel {
    /// The retail object: a setup (`0x02...`) or a graphics object (`0x01...`).
    pub id: DataId,
    /// The levels of detail, nearest first.
    pub lods: Vec<ManifestLod>,
    /// Cut-out growth: the model's cut-out materials (needles, leaves, grass) are drawn from both
    /// sides and lit through from behind as well as from the front. Its other materials (a
    /// trunk) are ordinary surfaces.
    pub foliage: bool,
    /// Which switch turns it on: the one it names, or the one it is taken for.
    pub category: Category,
    /// The fingerprint of the retail object it was made for, where it names one.
    pub fingerprint: Option<Fingerprint>,
    /// Which records it is drawn on.
    pub draws_on: DrawsOn,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

/// Which kind of thing a model is, for the switches that turn a pack's models on by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Tree,
    Plant,
    Rock,
    /// Growth in and by the water: lily pads, reeds. Never taken for; only named.
    Water,
}

impl Category {
    /// Every category, in order.
    pub const ALL: [Self; 4] = [Self::Tree, Self::Plant, Self::Rock, Self::Water];

    /// Its name in a manifest.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Plant => "plant",
            Self::Rock => "rock",
            Self::Water => "water",
        }
    }

    /// What a model that names no category is taken for: a tree when it has a level of
    /// pictures round it, a plant when it is foliage, a rock otherwise.
    #[must_use]
    pub fn taken_for(foliage: bool, lods: &[ManifestLod]) -> Self {
        if lods.iter().any(|l| l.views.is_some()) {
            Self::Tree
        } else if foliage {
            Self::Plant
        } else {
            Self::Rock
        }
    }
}

/// One level of detail.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestLod {
    /// The glTF 2.0 file, relative to the pack's folder.
    pub file: String,
    /// How far from the eye it is drawn, metres; `None` for the last level drawn at any
    /// distance.
    pub distance: Option<f32>,
    /// For a level that is pictures of the model seen from round it, how they are laid out.
    pub views: Option<Views>,
    /// Drawn into the sun's shadows alone, never into the view.
    pub shadows_only: bool,
}

/// How a level made of pictures of its model lays them out: how many views round the model, and
/// how many tiles to a row of its pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Views {
    /// The views, evenly round the model's upright axis.
    pub count: u32,
    /// The tiles to a row.
    pub columns: u32,
}

/// Who made a model, with what, and under which license.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Credit {
    /// The author.
    pub author: String,
    /// The tool or pipeline it was made with.
    pub tool: String,
    /// The license it is shared under.
    pub license: String,
}

/// Why a manifest was refused, as a sentence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("art pack manifest: {0}")]
pub struct ManifestError(pub String);

fn fail<T>(why: impl Into<String>) -> Result<T, ManifestError> {
    Err(ManifestError(why.into()))
}

impl ArtPackManifest {
    /// The manifest in `text`.
    ///
    /// # Errors
    /// [`ManifestError`] when the text is not TOML, its format is not [`FORMAT`], or a model is
    /// incomplete or contradicts another: an id that is not a setup or a graphics object, an id
    /// listed twice, no level of detail, a file path that is absolute or climbs out of the pack,
    /// distances that do not grow, a level before the last with no distance, or a credit left
    /// out; or a ground material is: a ground that is not a terrain type or the road, a ground
    /// two materials claim, no base color, or a size that is not a positive number; or a
    /// material for retail pictures is: a picture that is not a texture record, a picture two
    /// materials claim, no base color, or a picture path outside the pack.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let table: toml::Table = match text.parse() {
            Ok(t) => t,
            Err(e) => return fail(format!("not TOML: {e}")),
        };
        let Some(pack) = table.get("pack").and_then(toml::Value::as_table) else {
            return fail("no [pack] table");
        };
        match pack.get("format").and_then(toml::Value::as_integer) {
            Some(FORMAT) => {}
            Some(f) => return fail(format!("format {f}, and this client reads format {FORMAT}")),
            None => return fail("the [pack] table names no format"),
        }
        let name = pack
            .get("name")
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let about = about(pack)?;
        let empty = Vec::new();
        let listed = match table.get("model") {
            None => &empty,
            Some(toml::Value::Array(a)) => a,
            Some(_) => return fail("`model` is not a list of [[model]] tables"),
        };
        let mut models: Vec<ManifestModel> = Vec::with_capacity(listed.len());
        for (i, m) in listed.iter().enumerate() {
            let Some(m) = m.as_table() else {
                return fail(format!("model {} is not a table", i + 1));
            };
            let model = model(m).map_err(|e| ManifestError(format!("model {}: {}", i + 1, e.0)))?;
            if models.iter().any(|o| {
                o.id == model.id && (o.fingerprint.is_none() || o.fingerprint == model.fingerprint)
            }) {
                return fail(format!("{:#010X} is listed twice", model.id.0));
            }
            models.push(model);
        }
        let mut grounds: Vec<ManifestGround> = Vec::new();
        for (kind, steep) in [("terrain", false), ("steep", true)] {
            let listed = match table.get(kind) {
                None => &empty,
                Some(toml::Value::Array(a)) => a,
                Some(_) => return fail(format!("`{kind}` is not a list of [[{kind}]] tables")),
            };
            for (i, g) in listed.iter().enumerate() {
                let Some(g) = g.as_table() else {
                    return fail(format!("{kind} {} is not a table", i + 1));
                };
                let mut ground =
                    ground(g).map_err(|e| ManifestError(format!("{kind} {}: {}", i + 1, e.0)))?;
                ground.steep = steep;
                if steep && g.contains_key("pebbles") {
                    return fail(format!(
                        "{kind} {}: pebbles is named on the [[terrain]] material of its grounds",
                        i + 1
                    ));
                }
                for k in &ground.grounds {
                    if grounds.iter().any(|o| {
                        o.steep == steep
                            && o.grounds.contains(k)
                            && (o.fingerprint.is_none() || o.fingerprint == ground.fingerprint)
                    }) {
                        return fail(format!("{kind} {}: {k:?} has a material already", i + 1));
                    }
                }
                grounds.push(ground);
            }
        }
        let listed = match table.get("material") {
            None => &empty,
            Some(toml::Value::Array(a)) => a,
            Some(_) => return fail("`material` is not a list of [[material]] tables"),
        };
        // The materials for each picture so far, so a pack of thousands is checked quickly.
        let mut material_at: HashMap<DataId, Vec<usize>> = HashMap::new();
        let mut materials: Vec<ManifestMaterial> = Vec::with_capacity(listed.len());
        for (i, m) in listed.iter().enumerate() {
            let Some(m) = m.as_table() else {
                return fail(format!("material {} is not a table", i + 1));
            };
            let material =
                material(m).map_err(|e| ManifestError(format!("material {}: {}", i + 1, e.0)))?;
            for (t, print) in material.textures.iter().zip(&material.fingerprints) {
                let same = material_at.get(t).map_or(&[][..], Vec::as_slice);
                if same.iter().map(|&j| &materials[j]).any(|o| {
                    let apart = !o.palettes.is_empty()
                        && !material.palettes.is_empty()
                        && !o.palettes.iter().any(|p| material.palettes.contains(p));
                    !apart
                        && o.textures
                            .iter()
                            .zip(&o.fingerprints)
                            .any(|(u, p)| u == t && (p.is_none() || p == print))
                }) {
                    return fail(format!(
                        "material {}: {:#010X} has a material already",
                        i + 1,
                        t.0
                    ));
                }
            }
            for t in &material.textures {
                material_at.entry(*t).or_default().push(materials.len());
            }
            materials.push(material);
        }
        let listed = match table.get("cover") {
            None => &empty,
            Some(toml::Value::Array(a)) => a,
            Some(_) => return fail("`cover` is not a list of [[cover]] tables"),
        };
        let mut covers: Vec<ManifestCover> = Vec::with_capacity(listed.len());
        for (i, c) in listed.iter().enumerate() {
            let Some(c) = c.as_table() else {
                return fail(format!("cover {} is not a table", i + 1));
            };
            let (model, cover) =
                cover(c).map_err(|e| ManifestError(format!("cover {}: {}", i + 1, e.0)))?;
            // Covers that grow the same model on other types, or as thickly, share it.
            match models.iter().find(|o| o.id == model.id) {
                None => models.push(model),
                Some(o) if covers.iter().any(|c| c.id == o.id) && *o == model => {}
                Some(_) => {
                    return fail(format!(
                        "cover {}: another cover grows {:?} with other levels or credit",
                        i + 1,
                        model.lods[0].file
                    ))
                }
            }
            covers.push(cover);
        }
        let listed = match table.get("skin") {
            None => &empty,
            Some(toml::Value::Array(a)) => a,
            Some(_) => return fail("`skin` is not a list of [[skin]] tables"),
        };
        let mut skins: Vec<ManifestSkin> = Vec::with_capacity(listed.len());
        for (i, k) in listed.iter().enumerate() {
            let Some(k) = k.as_table() else {
                return fail(format!("skin {} is not a table", i + 1));
            };
            let one = skin(k).map_err(|e| ManifestError(format!("skin {}: {}", i + 1, e.0)))?;
            if skins.iter().any(|o| {
                o.setup == one.setup
                    && o.palettes.is_empty()
                    && one.palettes.is_empty()
                    && (o.fingerprint.is_none() || o.fingerprint == one.fingerprint)
            }) {
                return fail(format!(
                    "skin {}: {:#010X} has a skin already",
                    i + 1,
                    one.setup.0
                ));
            }
            skins.push(one);
        }
        let keep_launchers = match table.get("keep").and_then(|k| k.get("launchers")) {
            None => false,
            Some(toml::Value::Boolean(b)) => *b,
            Some(_) => return fail("keep: `launchers` is not true or false"),
        };
        let keep = match table.get("keep") {
            None => Vec::new(),
            Some(toml::Value::Table(k)) => match k.get("weenies") {
                None => Vec::new(),
                Some(toml::Value::Array(a)) => {
                    let mut out = Vec::with_capacity(a.len());
                    for v in a {
                        match v.as_integer().and_then(|w| u32::try_from(w).ok()) {
                            Some(w) if w > 0 => out.push(w),
                            _ => return fail(format!("keep: {v} is not a weenie's class id")),
                        }
                    }
                    out
                }
                Some(_) => return fail("keep: `weenies` is not a list of class ids"),
            },
            Some(_) => return fail("`keep` is not a [keep] table"),
        };
        let lighting = match table.get("lighting") {
            None => None,
            Some(toml::Value::Table(t)) => Some(t.to_string()),
            Some(_) => return fail("`lighting` is not a [lighting] table"),
        };
        Ok(Self {
            name,
            about,
            models,
            grounds,
            materials,
            covers,
            skins,
            keep,
            keep_launchers,
            lighting,
        })
    }

    /// The body drawn in place of the parts of `setup`, if the pack has one: the one for its
    /// objects whose look names none of the palettes the others are drawn for.
    #[must_use]
    pub fn skin(&self, setup: DataId) -> Option<&ManifestSkin> {
        self.skin_for(setup, &[])
    }

    /// The body drawn in place of the parts of an object built on `setup` whose look carries
    /// `palettes`: the first of the setup's bodies drawn for one of them, else its other one.
    #[must_use]
    pub fn skin_for(&self, setup: DataId, palettes: &[DataId]) -> Option<&ManifestSkin> {
        let mut rows = self.skins.iter().filter(|k| k.setup == setup);
        rows.clone()
            .find(|k| k.palettes.iter().any(|p| palettes.contains(p)))
            .or_else(|| rows.find(|k| k.palettes.is_empty()))
    }

    /// The material drawn in place of the retail picture `texture`, if the pack has one.
    #[must_use]
    pub fn material(&self, texture: DataId) -> Option<&ManifestMaterial> {
        self.materials
            .iter()
            .find(|m| m.textures.contains(&texture))
    }

    /// The material drawn for `ground`, if the pack has one.
    #[must_use]
    pub fn ground(&self, ground: Ground) -> Option<&ManifestGround> {
        self.grounds
            .iter()
            .find(|g| !g.steep && g.grounds.contains(&ground))
    }

    /// The material laid on `ground`'s steep slopes, if the pack has one.
    #[must_use]
    pub fn steep_ground(&self, ground: Ground) -> Option<&ManifestGround> {
        self.grounds
            .iter()
            .find(|g| g.steep && g.grounds.contains(&ground))
    }

    /// The model drawn in place of `id`, if the pack has one.
    #[must_use]
    pub fn model(&self, id: DataId) -> Option<&ManifestModel> {
        self.models.iter().find(|m| m.id == id)
    }
}

/// The id in `raw`: `0x` and eight hex digits, of a setup or a graphics object.
fn object_id(raw: &str) -> Result<DataId, ManifestError> {
    let digits = raw
        .trim()
        .strip_prefix("0x")
        .or_else(|| raw.trim().strip_prefix("0X"));
    let Some(id) = digits.and_then(|d| u32::from_str_radix(d, 16).ok()) else {
        return fail(format!("id {raw:?} is not 0x and hex digits"));
    };
    match id >> 24 {
        0x01 | 0x02 => Ok(DataId(id)),
        _ => fail(format!(
            "id {id:#010X} is neither a setup (0x02...) nor a graphics object (0x01...)"
        )),
    }
}

/// Whether `path` names a file inside the pack: relative, `/` between its parts, and no part
/// empty, `.` or `..`.
fn inside_the_pack(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn text<'t>(t: &'t toml::Table, key: &str) -> Result<&'t str, ManifestError> {
    match t.get(key).and_then(toml::Value::as_str).map(str::trim) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => fail(format!("no {key}")),
    }
}

/// The grounds `v` names: a terrain type, `"road"`, or a list of them.
fn grounds_of(v: Option<&toml::Value>) -> Result<Vec<Ground>, ManifestError> {
    let one = |v: &toml::Value| -> Result<Ground, ManifestError> {
        match v {
            toml::Value::Integer(t) => match u8::try_from(*t) {
                Ok(t) if t < 32 => Ok(Ground::Type(t)),
                _ => fail(format!("ground {t} is not a terrain type from 0 to 31")),
            },
            toml::Value::String(s) if s.trim().eq_ignore_ascii_case("road") => Ok(Ground::Road),
            other => fail(format!(
                "ground {other} is neither a terrain type nor \"road\""
            )),
        }
    };
    let grounds = match v {
        None => return fail("no ground"),
        Some(toml::Value::Array(a)) => a.iter().map(one).collect::<Result<Vec<_>, _>>()?,
        Some(v) => vec![one(v)?],
    };
    if grounds.is_empty() {
        return fail("no ground");
    }
    Ok(grounds)
}

fn ground(t: &toml::Table) -> Result<ManifestGround, ManifestError> {
    let grounds = grounds_of(t.get("ground"))?;
    let picture = |key: &str| -> Result<Option<String>, ManifestError> {
        match t.get(key) {
            None => Ok(None),
            Some(v) => match v.as_str().map(str::trim) {
                Some(p) if inside_the_pack(p) => Ok(Some(p.to_owned())),
                _ => fail(format!("{key} is not a path inside the pack")),
            },
        }
    };
    let Some(base_color) = picture("base_color")? else {
        return fail("no base_color");
    };
    let number = |key: &str| -> Result<Option<f32>, ManifestError> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        // Metres and strengths, well inside both types' range.
        match t.get(key) {
            None => Ok(None),
            Some(toml::Value::Float(f)) => Ok(Some(*f as f32)),
            Some(toml::Value::Integer(i)) => Ok(Some(*i as f32)),
            Some(_) => fail(format!("{key} is not a number")),
        }
    };
    let size = match number("size")? {
        Some(s) if s.is_finite() && s > 0.0 => s,
        Some(s) => return fail(format!("size {s} is not a positive number")),
        None => return fail("no size"),
    };
    let normal_scale = match number("normal_scale")? {
        Some(s) if s.is_finite() && s >= 0.0 => s,
        Some(s) => return fail(format!("normal_scale {s} is not a number from 0")),
        None => 1.0,
    };
    let detail_size = match number("detail_size")? {
        Some(s) if s.is_finite() && s > 0.0 => s,
        Some(s) => return fail(format!("detail_size {s} is not a positive number")),
        None => 1.0,
    };
    let detail_strength = match number("detail_strength")? {
        Some(s) if s.is_finite() && s >= 0.0 => s,
        Some(s) => return fail(format!("detail_strength {s} is not a number from 0")),
        None => 1.0,
    };
    let pebbles = match t.get("pebbles") {
        None => true,
        Some(v) => match v.as_bool() {
            Some(b) => b,
            None => return fail("pebbles is not true or false"),
        },
    };
    Ok(ManifestGround {
        grounds,
        base_color,
        normal: picture("normal")?,
        height: picture("height")?,
        roughness: picture("roughness")?,
        occlusion: picture("occlusion")?,
        size,
        normal_scale,
        detail: picture("detail")?,
        detail_size,
        detail_strength,
        pebbles,
        steep: false,
        fingerprint: fingerprints_of(t.get("fingerprint"), 1)?.remove(0),
        credit: Credit {
            author: text(t, "author")?.to_owned(),
            tool: text(t, "tool")?.to_owned(),
            license: text(t, "license")?.to_owned(),
        },
    })
}

/// The retail pictures `v` names: a texture record id (`0x05...`), or a list of them.
fn textures_of(v: Option<&toml::Value>) -> Result<Vec<DataId>, ManifestError> {
    let one = |v: &toml::Value| -> Result<DataId, ManifestError> {
        let Some(raw) = v.as_str() else {
            return fail(format!("texture {v} is not 0x and hex digits"));
        };
        let digits = raw
            .trim()
            .strip_prefix("0x")
            .or_else(|| raw.trim().strip_prefix("0X"));
        match digits.and_then(|d| u32::from_str_radix(d, 16).ok()) {
            Some(id) if id >> 24 == 0x05 => Ok(DataId(id)),
            Some(id) => fail(format!(
                "texture {id:#010X} is not a texture record (0x05...)"
            )),
            None => fail(format!("texture {raw:?} is not 0x and hex digits")),
        }
    };
    let textures = match v {
        None => return fail("no texture"),
        Some(toml::Value::Array(a)) => a.iter().map(one).collect::<Result<Vec<_>, _>>()?,
        Some(v) => vec![one(v)?],
    };
    if textures.is_empty() {
        return fail("no texture");
    }
    Ok(textures)
}

/// The fingerprints `v` names for `count` records: one text, or a list of one for each.
fn fingerprints_of(
    v: Option<&toml::Value>,
    count: usize,
) -> Result<Vec<Option<Fingerprint>>, ManifestError> {
    let one = |v: &toml::Value| -> Result<Fingerprint, ManifestError> {
        v.as_str()
            .and_then(Fingerprint::parse)
            .map_or_else(|| fail(format!("fingerprint {v} is not one")), Ok)
    };
    match v {
        None => Ok(vec![None; count]),
        Some(toml::Value::Array(a)) if a.len() == count => {
            a.iter().map(|v| one(v).map(Some)).collect()
        }
        Some(toml::Value::Array(_)) => fail(format!("fingerprint does not name {count}")),
        Some(v) if count == 1 => Ok(vec![Some(one(v)?)]),
        Some(_) => fail(format!("fingerprint does not name {count}")),
    }
}

/// The palettes `v` names: palette records (`0x04...`), one or a list.
fn palettes_of(v: Option<&toml::Value>) -> Result<Vec<DataId>, ManifestError> {
    let one = |v: &toml::Value| -> Result<DataId, ManifestError> {
        let id = v
            .as_str()
            .and_then(|s| s.trim().strip_prefix("0x"))
            .and_then(|d| u32::from_str_radix(d, 16).ok());
        match id {
            Some(id) if id >> 24 == 0x04 => Ok(DataId(id)),
            _ => fail(format!("palette {v} is not a palette record (0x04...)")),
        }
    };
    match v {
        None => Ok(Vec::new()),
        Some(toml::Value::Array(a)) => a.iter().map(one).collect(),
        Some(v) => Ok(vec![one(v)?]),
    }
}

/// The records an entry is drawn on, as its `draws_on` says.
fn draws_on(t: &toml::Table) -> Result<DrawsOn, ManifestError> {
    match t.get("draws_on").map(|v| v.as_str()) {
        None | Some(Some("matching")) => Ok(DrawsOn::Matching),
        Some(Some("any")) => Ok(DrawsOn::Any),
        Some(Some("none")) => Ok(DrawsOn::Nothing),
        Some(_) => fail("draws_on is not matching, any or none"),
    }
}

fn material(t: &toml::Table) -> Result<ManifestMaterial, ManifestError> {
    let textures = textures_of(t.get("texture"))?;
    let fingerprints = fingerprints_of(t.get("fingerprint"), textures.len())?;
    let flag = |key: &str| match t.get(key) {
        None => Ok(false),
        Some(toml::Value::Boolean(b)) => Ok(*b),
        Some(v) => fail(format!("{key} {v} is not true or false")),
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    // A share or an index, well inside both types' range.
    let number = |key: &str, lo: f64, hi: f64| match t.get(key) {
        None => Ok(None),
        Some(toml::Value::Float(f)) if (lo..=hi).contains(f) => Ok(Some(*f as f32)),
        Some(toml::Value::Integer(i)) if (lo..=hi).contains(&(*i as f64)) => Ok(Some(*i as f32)),
        Some(v) => fail(format!("{key} {v} is not a number from {lo} to {hi}")),
    };
    let matter = Matter {
        metal: flag("metal")?.then_some(number("brighten", 0.1, 20.0)?.unwrap_or(1.0)),
        gloss: number("gloss", 0.0, 1.0)?,
        clear: number("clear", 0.0, 1.0)?,
        refraction: number("refraction", 1.0, 3.0)?.unwrap_or(1.31),
        wet: number("wet", 0.0, 1.0)?,
    };
    let picture = |key: &str| -> Result<Option<String>, ManifestError> {
        match t.get(key) {
            None => Ok(None),
            Some(v) => match v.as_str().map(str::trim) {
                Some(p) if inside_the_pack(p) => Ok(Some(p.to_owned())),
                _ => fail(format!("{key} is not a path inside the pack")),
            },
        }
    };
    let Some(base_color) = picture("base_color")? else {
        return fail("no base_color");
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    // A strength, well inside both types' range.
    let normal_scale = match t.get("normal_scale") {
        None => 1.0,
        Some(toml::Value::Float(f)) if f.is_finite() && *f >= 0.0 => *f as f32,
        Some(toml::Value::Integer(i)) if *i >= 0 => *i as f32,
        Some(v) => return fail(format!("normal_scale {v} is not a number from 0")),
    };
    Ok(ManifestMaterial {
        textures,
        base_color,
        normal: picture("normal")?,
        roughness: picture("roughness")?,
        occlusion: picture("occlusion")?,
        height: picture("height")?,
        normal_scale,
        keep_retail_picture: flag("keep_retail_picture")?,
        damp: number("damp", 0.0, 1.0)?.unwrap_or(0.0),
        damp_map: picture("damp_map")?,
        matter,
        fingerprints,
        draws_on: draws_on(t)?,
        palettes: palettes_of(t.get("palettes"))?,
        credit: Credit {
            author: text(t, "author")?.to_owned(),
            tool: text(t, "tool")?.to_owned(),
            license: text(t, "license")?.to_owned(),
        },
    })
}

fn skin(t: &toml::Table) -> Result<ManifestSkin, ManifestError> {
    let setup = object_id(t.get("setup").and_then(toml::Value::as_str).unwrap_or(""))?;
    if setup.0 >> 24 != 0x02 {
        return fail(format!("{:#010X} is not a setup (0x02...)", setup.0));
    }
    let file = text(t, "file")?;
    if !inside_the_pack(file) {
        return fail(format!("file {file:?} is not a path inside the pack"));
    }
    let palettes = match t.get("palettes") {
        None => Vec::new(),
        Some(toml::Value::Array(a)) => {
            let mut out = Vec::with_capacity(a.len());
            for v in a {
                let id = v.as_str().and_then(|v| {
                    let d = v.trim().strip_prefix("0x")?;
                    u32::from_str_radix(d, 16).ok()
                });
                match id {
                    Some(id) if id >> 24 == 0x04 => out.push(DataId(id)),
                    _ => return fail(format!("palette {v} is not a palette (0x04...)")),
                }
            }
            out
        }
        Some(_) => return fail("`palettes` is not a list of palette ids"),
    };
    let mut clips = Vec::new();
    let mut by_animation = Vec::new();
    match t.get("clips") {
        None => {}
        Some(toml::Value::Table(c)) => {
            for (k, v) in c {
                let Some(name) = v.as_str() else {
                    return fail(format!("clip {k} = {v} is not an animation's name"));
                };
                if let Some(d) = k.trim().strip_prefix("0x") {
                    match u32::from_str_radix(d, 16) {
                        Ok(id) if id >> 24 == 0x03 => {
                            by_animation.push((DataId(id), name.to_owned()));
                        }
                        _ => return fail(format!("{k} is not an animation (0x03...)")),
                    }
                } else if CLIP_SLOTS.contains(&k.as_str()) {
                    clips.push((k.clone(), name.to_owned()));
                } else {
                    return fail(format!("clip {k} is not a slot"));
                }
            }
        }
        Some(_) => return fail("`clips` is not a table of slots and animation names"),
    }
    Ok(ManifestSkin {
        setup,
        file: file.to_owned(),
        palettes,
        clips,
        by_animation,
        fingerprint: fingerprints_of(t.get("fingerprint"), 1)?.remove(0),
        draws_on: draws_on(t)?,
        credit: Credit {
            author: text(t, "author")?.to_owned(),
            tool: text(t, "tool")?.to_owned(),
            license: text(t, "license")?.to_owned(),
        },
    })
}

fn model(t: &toml::Table) -> Result<ManifestModel, ManifestError> {
    let id = object_id(t.get("id").and_then(toml::Value::as_str).unwrap_or(""))?;
    Ok(ManifestModel {
        fingerprint: fingerprints_of(t.get("fingerprint"), 1)?.remove(0),
        draws_on: draws_on(t)?,
        ..shape(t, id, "model")?
    })
}

/// A cover's model, listed under its first file's id, and how it grows.
fn cover(t: &toml::Table) -> Result<(ManifestModel, ManifestCover), ManifestError> {
    if t.contains_key("id") {
        return fail("a cover is no retail object's, and names no id");
    }
    let mut model = shape(t, DataId(COVER_ID), "cover")?;
    model.id = cover_id(&model.lods[0].file);
    let Some(reach) = model
        .lods
        .iter()
        .rev()
        .find(|l| !l.shadows_only)
        .and_then(|l| l.distance)
    else {
        return fail("its last level names no distance, how far from the eye it grows");
    };
    if model.lods.iter().any(|l| l.views.is_some()) {
        return fail("a cover has no level of pictures round it");
    }
    let grounds = grounds_of(t.get("ground"))?
        .into_iter()
        .map(|g| match g {
            Ground::Type(t) => Ok(t),
            Ground::Road => fail("a cover grows on terrain types, not on the road"),
        })
        .collect::<Result<Vec<_>, _>>()?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    // Counts, sizes and slopes, well inside both types' range.
    let number = |v: &toml::Value| match v {
        toml::Value::Float(f) => Some(*f as f32),
        toml::Value::Integer(i) => Some(*i as f32),
        _ => None,
    };
    let density = match t.get("density").map(number) {
        Some(Some(d)) if d.is_finite() && d > 0.0 => d,
        Some(_) => return fail("density is not a positive number"),
        None => return fail("no density"),
    };
    let scale = match t.get("scale") {
        None => (1.0, 1.0),
        Some(toml::Value::Array(a)) if a.len() == 2 => match (number(&a[0]), number(&a[1])) {
            (Some(lo), Some(hi)) if lo.is_finite() && lo > 0.0 && hi.is_finite() && hi >= lo => {
                (lo, hi)
            }
            _ => return fail("scale is not two positive numbers, the least first"),
        },
        Some(_) => return fail("scale is not two positive numbers, the least first"),
    };
    let max_slope = match t.get("max_slope").map(number) {
        None => 0.6,
        Some(Some(s)) if s.is_finite() && s >= 0.0 => s,
        Some(_) => return fail("max_slope is not a number from 0"),
    };
    let cover = ManifestCover {
        id: model.id,
        grounds,
        density,
        scale,
        max_slope,
        reach,
    };
    Ok((model, cover))
}

/// A model's levels, how it is lit, which switch turns it on and its credit, from `t`, a
/// `[[kind]]` table, listed under `id`.
fn shape(t: &toml::Table, id: DataId, kind: &str) -> Result<ManifestModel, ManifestError> {
    let foliage = match t.get("foliage") {
        None => false,
        Some(v) => match v.as_bool() {
            Some(b) => b,
            None => return fail("foliage is not true or false"),
        },
    };
    let credit = Credit {
        author: text(t, "author")?.to_owned(),
        tool: text(t, "tool")?.to_owned(),
        license: text(t, "license")?.to_owned(),
    };
    let Some(levels) = t.get("lod").and_then(toml::Value::as_array) else {
        return fail(format!("no [[{kind}.lod]] level of detail"));
    };
    if levels.is_empty() {
        return fail(format!("no [[{kind}.lod]] level of detail"));
    }
    let mut lods: Vec<ManifestLod> = Vec::with_capacity(levels.len());
    for (k, l) in levels.iter().enumerate() {
        let Some(l) = l.as_table() else {
            return fail(format!("level {k} is not a table"));
        };
        let file = text(l, "file")?;
        if !inside_the_pack(file) {
            return fail(format!(
                "level {k}'s file {file:?} is not a path inside the pack"
            ));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        // A distance in metres, well inside both types' range.
        let distance = match l.get("distance") {
            None => None,
            Some(toml::Value::Float(f)) => Some(*f as f32),
            Some(toml::Value::Integer(i)) => Some(*i as f32),
            Some(_) => return fail(format!("level {k}'s distance is not a number")),
        };
        let shadows_only = match l.get("shadows_only") {
            None => false,
            Some(v) => match v.as_bool() {
                Some(b) => b,
                None => return fail(format!("level {k}'s shadows_only is not true or false")),
            },
        };
        if shadows_only && (k == 0 || k + 1 == levels.len()) {
            return fail(format!(
                "level {k} is for the shadows only, and the first and the last level are drawn"
            ));
        }
        // A level for the shadows alone has no distance of its own in the view.
        let distance = distance.filter(|_| !shadows_only);
        if let Some(d) = distance {
            if !(d.is_finite() && d > 0.0) {
                return fail(format!("level {k}'s distance {d} is not a positive number"));
            }
            if let Some(before) = lods
                .iter()
                .rev()
                .find(|p| !p.shadows_only)
                .and_then(|p| p.distance)
            {
                if d <= before {
                    return fail(format!(
                        "level {k}'s distance {d} does not reach past the level before it"
                    ));
                }
            }
        }
        if !shadows_only
            && lods
                .iter()
                .rev()
                .find(|p| !p.shadows_only)
                .is_some_and(|p| p.distance.is_none())
        {
            return fail(format!(
                "a level before level {k} names no distance, and only the last level may leave \
                 it out"
            ));
        }
        let count = |key: &str| -> Result<Option<u32>, ManifestError> {
            match l.get(key) {
                None => Ok(None),
                Some(v) => match v.as_integer().and_then(|n| u32::try_from(n).ok()) {
                    Some(n) if n >= 1 => Ok(Some(n)),
                    _ => fail(format!("level {k}'s {key} is not a whole number from 1")),
                },
            }
        };
        let views = match (count("views")?, count("columns")?) {
            (None, None) => None,
            (None, Some(_)) => return fail(format!("level {k} names columns and no views")),
            (Some(count), columns) => {
                if count < 2 {
                    return fail(format!("level {k} names fewer than two views"));
                }
                Some(Views {
                    count,
                    columns: columns.unwrap_or(count).min(count),
                })
            }
        };
        lods.push(ManifestLod {
            file: file.to_owned(),
            distance,
            views,
            shadows_only,
        });
    }
    let category = match t.get("category") {
        None => Category::taken_for(foliage, &lods),
        Some(v) => match v.as_str().map(str::trim).and_then(|name| {
            Category::ALL
                .into_iter()
                .find(|c| c.name().eq_ignore_ascii_case(name))
        }) {
            Some(c) => c,
            None => {
                return fail(format!(
                    "category {v} is not \"tree\", \"plant\", \"rock\" or \"water\""
                ))
            }
        },
    };
    Ok(ManifestModel {
        id,
        lods,
        foliage,
        category,
        fingerprint: None,
        draws_on: DrawsOn::Matching,
        credit,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    const PILOT: &str = r#"
[pack]
name = "Pilot"
format = 1

[[model]]
id = "0x02000258"
foliage = true
author = "Someone"
tool = "A generator"
license = "CC0-1.0"

[[model.lod]]
file = "tree/lod0.gltf"
distance = 40.0

[[model.lod]]
file = "tree/lod1.gltf"
distance = 120

[[model.lod]]
file = "tree/lod2.gltf"

[[model]]
id = "0x01000A7E"
author = "Someone"
tool = "A generator"
license = "CC0-1.0"

[[model.lod]]
file = "rock.gltf"
"#;

    #[test]
    fn a_manifest_lists_each_model_with_its_levels_its_lighting_and_its_credit() {
        let m = ArtPackManifest::parse(PILOT).expect("the pilot manifest reads");
        assert_eq!(m.name, "Pilot");
        assert_eq!(m.models.len(), 2);
        let tree = m.model(DataId(0x0200_0258)).expect("the tree");
        assert!(tree.foliage);
        assert_eq!(
            tree.lods
                .iter()
                .map(|l| (l.file.as_str(), l.distance))
                .collect::<Vec<_>>(),
            [
                ("tree/lod0.gltf", Some(40.0)),
                ("tree/lod1.gltf", Some(120.0)),
                ("tree/lod2.gltf", None),
            ]
        );
        assert_eq!(tree.credit.license, "CC0-1.0");
        let rock = m
            .model(DataId(0x0100_0A7E))
            .expect("a graphics object's model");
        assert!(!rock.foliage, "foliage is off unless the manifest says so");
        assert_eq!(rock.lods.len(), 1);
        assert!(m.model(DataId(0x0200_0001)).is_none());
        assert!(tree.lods.iter().all(|l| l.views.is_none()));
    }

    #[test]
    fn a_skin_names_the_setup_it_is_drawn_for_its_file_and_its_credit() {
        let body = |setup: &str, file: &str| {
            format!(
                "{PILOT}
[[skin]]
setup = \"{setup}\"
file = \"{file}\"
author = \"A\"
                 tool = \"B\"
license = \"CC0-1.0\"
"
            )
        };
        let m = ArtPackManifest::parse(&body("0x02000001", "human/body.gltf")).expect("reads");
        let k = m.skin(DataId(0x0200_0001)).expect("the human's body");
        assert_eq!(k.file, "human/body.gltf");
        assert_eq!(k.credit.tool, "B");
        assert!(m.skin(DataId(0x0200_0002)).is_none());
        assert!(ArtPackManifest::parse(PILOT)
            .expect("reads")
            .skins
            .is_empty());
        // Only a setup, a file inside the pack, and one skin a setup.
        assert!(ArtPackManifest::parse(&body("0x01000001", "b.gltf")).is_err());
        assert!(ArtPackManifest::parse(&body("0x02000001", "../b.gltf")).is_err());
        let twice = format!(
            "{}
[[skin]]
setup = \"0x02000001\"
file = \"c.gltf\"
author = \"A\"
             tool = \"B\"
license = \"C\"
",
            body("0x02000001", "b.gltf")
        );
        assert!(ArtPackManifest::parse(&twice).is_err());
    }

    /// A setup takes a body for each era's record of it, told apart by fingerprint; the same
    /// record twice, or a body for any record before one for a single record, is refused.
    #[test]
    fn a_setup_takes_a_body_for_each_eras_record_of_it() {
        let body = |extra: &str| {
            format!(
                "[[skin]]\nsetup = \"0x02000001\"\nfile = \"b.gltf\"\n{extra}\
                 author = \"A\"\ntool = \"B\"\nlicense = \"C\"\n"
            )
        };
        let (eor, old) = (
            "fingerprint = \"model 0123456789abcdef\"\n",
            "fingerprint = \"model fedcba9876543210\"\n",
        );
        let m = ArtPackManifest::parse(&format!("{PILOT}\n{}{}{}", body(eor), body(old), body("")))
            .expect("a body for each era, then one for any");
        assert_eq!(m.skins.len(), 3);
        assert_eq!(
            m.skins[1].fingerprint,
            Fingerprint::parse("model fedcba9876543210")
        );
        assert!(m.skins[2].fingerprint.is_none());
        assert_eq!(m.skins[0].draws_on, DrawsOn::Matching);
        for (pair, why) in [
            ((eor, eor), "one record twice"),
            (("", eor), "any record, then one"),
        ] {
            let text = format!("{PILOT}\n{}{}", body(pair.0), body(pair.1));
            let e = ArtPackManifest::parse(&text).expect_err(why);
            assert!(e.0.contains("has a skin already"), "{why}: {}", e.0);
        }
        let any = ArtPackManifest::parse(&format!("{PILOT}\n{}", body("draws_on = \"any\"\n")))
            .expect("draws_on reads");
        assert_eq!(any.skins[0].draws_on, DrawsOn::Any);
        assert!(
            ArtPackManifest::parse(&format!("{PILOT}\n{}", body("draws_on = \"some\"\n"))).is_err()
        );
        assert!(
            ArtPackManifest::parse(&format!("{PILOT}\n{}", body("fingerprint = \"model\"\n")))
                .is_err()
        );
    }

    #[test]
    fn a_pack_lists_the_weenies_that_keep_their_own_parts_and_whether_launchers_do() {
        let m = ArtPackManifest::parse(&format!("{PILOT}\n[keep]\nweenies = [7, 237, 19196]\n"))
            .expect("reads");
        assert_eq!(m.keep, [7, 237, 19196]);
        assert!(!m.keep_launchers);
        let plain = ArtPackManifest::parse(PILOT).expect("reads");
        assert!(plain.keep.is_empty() && !plain.keep_launchers);
        let launchers =
            ArtPackManifest::parse(&format!("{PILOT}\n[keep]\nlaunchers = true\n")).expect("reads");
        assert!(launchers.keep_launchers && launchers.keep.is_empty());
        // Only class ids, in a list under [keep].
        for bad in [
            "[keep]\nweenies = [0]",
            "[keep]\nweenies = [-3]",
            "[keep]\nweenies = [\"7\"]",
            "[keep]\nweenies = 7",
            "[[keep]]\nweenies = [7]",
            "[keep]\nlaunchers = 1",
        ] {
            assert!(
                ArtPackManifest::parse(&format!("{PILOT}\n{bad}\n")).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_setup_s_bodies_are_chosen_by_the_palettes_an_object_carries() {
        let m = ArtPackManifest::parse(&format!(
            "{PILOT}
[[skin]]
setup = \"0x02000001\"
file = \"red.gltf\"
palettes = [\"0x04000010\", \"0x04000011\"]
author = \"A\"
tool = \"B\"
license = \"C\"

[[skin]]
setup = \"0x02000001\"
file = \"plain.gltf\"
author = \"A\"
tool = \"B\"
license = \"C\"
"
        ))
        .expect("reads");
        let file = |p: &[u32]| {
            let p: Vec<DataId> = p.iter().copied().map(DataId).collect();
            m.skin_for(DataId(0x0200_0001), &p).map(|k| k.file.as_str())
        };
        assert_eq!(file(&[0x0400_0011, 0x0400_0099]), Some("red.gltf"));
        assert_eq!(file(&[0x0400_0099]), Some("plain.gltf"));
        assert_eq!(file(&[]), Some("plain.gltf"));
        assert_eq!(
            m.skin(DataId(0x0200_0001)).map(|k| k.file.as_str()),
            Some("plain.gltf")
        );
        // A palette list names palettes only.
        let wrong = format!(
            "{PILOT}
[[skin]]
setup = \"0x02000001\"
file = \"red.gltf\"
palettes = [\"0x05000010\"]
author = \"A\"
tool = \"B\"
license = \"C\"
"
        );
        assert!(ArtPackManifest::parse(&wrong).is_err());
    }

    #[test]
    fn a_body_lists_its_own_clips_by_slot_and_opens_a_table_by_animation_as_packs_once_had() {
        let row = |clips: &str| {
            format!(
                "{PILOT}
[[skin]]
setup = \"0x02000001\"
file = \"b.gltf\"
clips = {clips}
author = \"A\"
tool = \"B\"
license = \"C\"
"
            )
        };
        let m = ArtPackManifest::parse(&row(
            "{ idle = \"Idle\", melee_high = \"Melee_Strong1\", die = \"death\" }",
        ))
        .expect("reads");
        let mut clips = m.skins[0].clips.clone();
        clips.sort();
        assert_eq!(
            clips,
            vec![
                ("die".to_owned(), "death".to_owned()),
                ("idle".to_owned(), "Idle".to_owned()),
                ("melee_high".to_owned(), "Melee_Strong1".to_owned())
            ]
        );
        assert!(m.skins[0].by_animation.is_empty());
        let m = ArtPackManifest::parse(&row(
            "{ \"0x03000044\" = \"run\", \"0x03000043\" = \"attack\" }",
        ))
        .expect("reads");
        let mut old = m.skins[0].by_animation.clone();
        old.sort();
        assert_eq!(
            old,
            vec![
                (DataId(0x0300_0043), "attack".to_owned()),
                (DataId(0x0300_0044), "run".to_owned())
            ]
        );
        assert!(m.skins[0].clips.is_empty());
        // A slot the format has not.
        assert!(ArtPackManifest::parse(&row("{ dance = \"Dance\" }")).is_err());
        // A key the format no longer reads is passed over.
        let old = row("{}").replacen("clips = {}", "clips = {}\nheld = false", 1);
        assert!(ArtPackManifest::parse(&old).is_ok());
        // The keys are animations, the values names.
        assert!(ArtPackManifest::parse(&row("{ \"0x05000044\" = \"run\" }")).is_err());
        assert!(ArtPackManifest::parse(&row("{ \"0x03000044\" = 1 }")).is_err());
        assert!(ArtPackManifest::parse(&row("[\"run\"]")).is_err());
    }

    #[test]
    fn a_level_of_pictures_round_the_model_names_its_views_and_their_rows() {
        let far = PILOT.replacen(
            "file = \"tree/lod2.gltf\"",
            "file = \"tree/far.gltf\"\nviews = 16\ncolumns = 8",
            1,
        );
        let m = ArtPackManifest::parse(&far).expect("reads");
        let tree = m.model(DataId(0x0200_0258)).expect("the tree");
        assert_eq!(
            tree.lods[2].views,
            Some(Views {
                count: 16,
                columns: 8
            })
        );
        // All in one row when it names no columns.
        let one_row = PILOT.replacen(
            "file = \"tree/lod2.gltf\"",
            "file = \"tree/far.gltf\"\nviews = 12",
            1,
        );
        let m = ArtPackManifest::parse(&one_row).expect("reads");
        assert_eq!(
            m.model(DataId(0x0200_0258)).expect("the tree").lods[2].views,
            Some(Views {
                count: 12,
                columns: 12
            })
        );
        for bad in [
            "views = 1",
            "views = 0",
            "views = \"many\"",
            "columns = 4",
            "shadows_only = true",
        ] {
            let text = PILOT.replacen(
                "file = \"tree/lod2.gltf\"",
                &format!("file = \"tree/far.gltf\"\n{bad}"),
                1,
            );
            assert!(ArtPackManifest::parse(&text).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_manifest_that_contradicts_itself_or_leaves_something_out_is_refused() {
        let with = |from: &str, to: &str| {
            assert!(PILOT.contains(from), "{from}");
            ArtPackManifest::parse(&PILOT.replacen(from, to, 1))
        };
        for (from, to) in [
            ("format = 1", "format = 2"),
            ("format = 1", ""),
            ("[pack]", "[box]"),
            ("0x02000258", "0x08000258"),
            ("0x02000258", "02000258"),
            ("0x01000A7E", "0x02000258"),
            ("tree/lod0.gltf", "/tree/lod0.gltf"),
            ("tree/lod0.gltf", "../tree/lod0.gltf"),
            ("tree/lod0.gltf", "C:/tree/lod0.gltf"),
            ("tree/lod0.gltf", "tree\\\\lod0.gltf"),
            ("distance = 120", "distance = 30"),
            ("distance = 120", "distance = -5"),
            ("distance = 40.0", ""),
            ("foliage = true", "foliage = \"yes\""),
            ("license = \"CC0-1.0\"", "license = \"\""),
            ("author = \"Someone\"", ""),
        ] {
            assert!(with(from, to).is_err(), "{from} -> {to}");
        }
        assert!(ArtPackManifest::parse("not toml at all [").is_err());
        let bare = "[pack]\nformat = 1\n[[model]]\nid = \"0x02000001\"\nauthor = \"a\"\n\
                    tool = \"t\"\nlicense = \"l\"\n";
        assert!(
            ArtPackManifest::parse(bare).is_err(),
            "a model with no level of detail"
        );
    }

    #[test]
    fn a_level_for_the_shadows_alone_is_kept_out_of_the_views_distances() {
        let shadows = PILOT.replacen(
            "file = \"tree/lod1.gltf\"\ndistance = 120",
            "file = \"tree/lod1.gltf\"\nshadows_only = true",
            1,
        );
        let m = ArtPackManifest::parse(&shadows).expect("reads");
        let tree = m.model(DataId(0x0200_0258)).expect("the tree");
        assert_eq!(
            tree.lods
                .iter()
                .map(|l| (l.distance, l.shadows_only))
                .collect::<Vec<_>>(),
            [(Some(40.0), false), (None, true), (None, false)]
        );
        // Its distance, when it names one, is not read, and the next level is measured against
        // the one before it.
        let named = PILOT.replacen(
            "file = \"tree/lod1.gltf\"\ndistance = 120",
            "file = \"tree/lod1.gltf\"\ndistance = 10\nshadows_only = true",
            1,
        );
        assert_eq!(
            ArtPackManifest::parse(&named).expect("reads").models[0].lods[1].distance,
            None
        );
        let first = PILOT.replacen("distance = 40.0", "distance = 40.0\nshadows_only = true", 1);
        assert!(ArtPackManifest::parse(&first).is_err(), "the first level");
    }

    #[test]
    fn a_manifest_with_no_models_is_an_empty_pack() {
        let m = ArtPackManifest::parse("[pack]\nformat = 1\n").expect("reads");
        assert!(m.models.is_empty());
        assert_eq!(m.name, "");
    }

    const GROUND: &str = r#"
[pack]
name = "Ground"
format = 1

[[terrain]]
ground = [1, 9]
base_color = "ground/grass_albedo.png"
normal = "ground/grass_normal.png"
height = "ground/grass_height.png"
size = 4
author = "a"
tool = "t"
license = "CC0-1.0"

[[terrain]]
ground = "road"
base_color = "ground/road_albedo.png"
size = 8.5
normal_scale = 0.5
detail = "ground/grain.png"
detail_size = 3
detail_strength = 0.4
pebbles = false
author = "a"
tool = "t"
license = "CC0-1.0"
"#;

    #[test]
    fn a_manifest_lays_a_ground_material_for_each_terrain_type_it_names_and_the_road() {
        let m = ArtPackManifest::parse(GROUND).expect("the ground manifest reads");
        assert!(m.models.is_empty());
        assert_eq!(m.grounds.len(), 2);
        let grass = m.ground(Ground::Type(9)).expect("type 9's material");
        assert_eq!(grass.grounds, [Ground::Type(1), Ground::Type(9)]);
        assert_eq!(grass.normal.as_deref(), Some("ground/grass_normal.png"));
        assert_eq!(grass.roughness, None);
        assert!((grass.size - 4.0).abs() < 1e-6);
        assert!(
            (grass.normal_scale - 1.0).abs() < 1e-6,
            "a full bend unless named"
        );
        assert_eq!(grass.detail, None);
        assert!(
            (grass.detail_strength - 1.0).abs() < 1e-6,
            "the detail as made unless named"
        );
        assert!(grass.pebbles, "the pebbles lie unless named");
        let road = m.ground(Ground::Road).expect("the road's material");
        assert!((road.size - 8.5).abs() < 1e-6);
        assert!((road.normal_scale - 0.5).abs() < 1e-6);
        assert_eq!(road.detail.as_deref(), Some("ground/grain.png"));
        assert!((road.detail_size - 3.0).abs() < 1e-6);
        assert!((road.detail_strength - 0.4).abs() < 1e-6);
        assert!(!road.pebbles);
        let unnamed =
            ArtPackManifest::parse(&GROUND.replace("detail_size = 3\n", "")).expect("reads");
        let road = unnamed.ground(Ground::Road).expect("the road's material");
        assert!(
            (road.detail_size - 1.0).abs() < 1e-6,
            "a meter a repeat unless named"
        );
        assert!(
            m.ground(Ground::Type(3)).is_none(),
            "an unnamed type keeps retail's"
        );
        for (from, to, why) in [
            ("ground = [1, 9]", "ground = [1, 32]", "not a terrain type"),
            (
                "ground = [1, 9]",
                "ground = [1, \"river\"]",
                "neither a terrain type",
            ),
            (
                "ground = \"road\"",
                "ground = [9]",
                "has a material already",
            ),
            ("size = 4\n", "size = 0\n", "not a positive number"),
            ("size = 4\n", "", "no size"),
            (
                "detail_size = 3\n",
                "detail_size = -1\n",
                "detail_size -1 is not a positive number",
            ),
            (
                "detail = \"ground/grain.png\"",
                "detail = \"/grain.png\"",
                "detail is not a path inside the pack",
            ),
            (
                "detail_strength = 0.4\n",
                "detail_strength = -0.5\n",
                "detail_strength -0.5 is not a number from 0",
            ),
            (
                "pebbles = false\n",
                "pebbles = 0\n",
                "pebbles is not true or false",
            ),
            (
                "base_color = \"ground/grass_albedo.png\"",
                "",
                "no base_color",
            ),
            (
                "normal = \"ground/grass_normal.png\"",
                "normal = \"../x.png\"",
                "inside the pack",
            ),
            (
                "license = \"CC0-1.0\"\n\n[[terrain]]",
                "\n[[terrain]]",
                "no license",
            ),
        ] {
            let text = GROUND.replacen(from, to, 1);
            assert_ne!(text, GROUND, "{from}");
            let e = ArtPackManifest::parse(&text).expect_err(why);
            assert!(e.0.contains(why), "{why}: {}", e.0);
        }
    }

    const MATERIALS: &str = r#"
[pack]
name = "Materials"
format = 1

[[material]]
texture = "0x0500287C"
base_color = "materials/wall_albedo.png"
normal = "materials/wall_normal.png"
roughness = "materials/wall_roughness.png"
author = "a"
tool = "t"
license = "CC0-1.0"

[[material]]
texture = ["0x05002752", "0x0500274E"]
base_color = "materials/floor_albedo.png"
normal_scale = 0.5
author = "a"
tool = "t"
license = "CC0-1.0"
"#;

    #[test]
    fn a_steep_material_is_laid_on_its_grounds_steep_slopes_and_not_on_their_flat_ground() {
        let text = format!(
            "{GROUND}\n[[steep]]\nground = [9, 13]\n\
             base_color = \"ground/scrub_steep_albedo.png\"\nsize = 8\n\
             author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n"
        );
        let m = ArtPackManifest::parse(&text).expect("the manifest reads");
        assert_eq!(m.grounds.len(), 3);
        let flat = m.ground(Ground::Type(9)).expect("type 9's material");
        assert!(!flat.steep);
        assert_eq!(flat.base_color, "ground/grass_albedo.png");
        let steep = m
            .steep_ground(Ground::Type(9))
            .expect("type 9's steep material");
        assert!(steep.steep);
        assert_eq!(steep.base_color, "ground/scrub_steep_albedo.png");
        assert!(m.steep_ground(Ground::Type(13)).is_some());
        assert!(
            m.ground(Ground::Type(13)).is_none(),
            "a type with a steep material alone keeps retail's flat ground"
        );
        assert!(m.steep_ground(Ground::Type(1)).is_none());
        let bad = text.replace("ground = [9, 13]", "ground = [9, 40]");
        assert!(ArtPackManifest::parse(&bad).is_err());
        let strength = text.replace("size = 8\n", "size = 8\ndetail_strength = 2\n");
        let m = ArtPackManifest::parse(&strength).expect("reads");
        let steep = m.steep_ground(Ground::Type(9)).expect("type 9's steep");
        assert!((steep.detail_strength - 2.0).abs() < 1e-6);
        let pebbles = text.replace("size = 8\n", "size = 8\npebbles = false\n");
        let e = ArtPackManifest::parse(&pebbles).expect_err("pebbles on a steep material");
        assert!(
            e.0.contains("steep 1: pebbles is named on the [[terrain]]"),
            "{}",
            e.0
        );
        let twice = format!(
            "{text}
[[steep]]
ground = 13
base_color = \"b.png\"
size = 8
author = \"a\"
tool = \"t\"
license = \"CC0-1.0\"
"
        );
        assert!(
            ArtPackManifest::parse(&twice).is_err(),
            "one steep material a type"
        );
    }

    #[test]
    fn a_manifest_names_a_material_for_each_retail_picture_it_stands_for() {
        let m = ArtPackManifest::parse(MATERIALS).expect("the materials manifest reads");
        assert!(m.models.is_empty() && m.grounds.is_empty());
        assert_eq!(m.materials.len(), 2);
        let wall = m
            .material(DataId(0x0500_287C))
            .expect("the wall's material");
        assert_eq!(wall.base_color, "materials/wall_albedo.png");
        assert_eq!(wall.normal.as_deref(), Some("materials/wall_normal.png"));
        assert_eq!(wall.occlusion, None);
        assert!(
            (wall.normal_scale - 1.0).abs() < 1e-6,
            "a full bend unless named"
        );
        let floor = m
            .material(DataId(0x0500_274E))
            .expect("the floor's material");
        assert_eq!(floor.textures, [DataId(0x0500_2752), DataId(0x0500_274E)]);
        assert!((floor.normal_scale - 0.5).abs() < 1e-6);
        assert!(
            m.material(DataId(0x0500_1CB4)).is_none(),
            "an unnamed picture keeps retail's"
        );
        for (from, to, why) in [
            ("\"0x0500287C\"", "\"0x0800287C\"", "not a texture record"),
            ("\"0x0500287C\"", "\"0500287C\"", "not 0x and hex digits"),
            ("\"0x0500287C\"", "[]", "no texture"),
            (
                "\"0x0500274E\"]",
                "\"0x0500287C\"]",
                "has a material already",
            ),
            (
                "base_color = \"materials/wall_albedo.png\"",
                "",
                "no base_color",
            ),
            (
                "normal = \"materials/wall_normal.png\"",
                "normal = \"/x.png\"",
                "inside the pack",
            ),
            (
                "normal_scale = 0.5",
                "normal_scale = -1",
                "not a number from 0",
            ),
            ("author = \"a\"", "", "no author"),
        ] {
            let text = MATERIALS.replacen(from, to, 1);
            assert_ne!(text, MATERIALS, "{from}");
            let e = ArtPackManifest::parse(&text).expect_err(why);
            assert!(e.0.contains(why), "{why}: {}", e.0);
        }
    }

    /// A model's category is the one it names, or a tree when it has a level of pictures round
    /// it, a plant when it is foliage, a rock otherwise; a water model only when named; a
    /// category that is none of the four is refused.
    #[test]
    fn a_models_category_is_named_or_taken_from_its_levels_and_foliage() {
        let m = ArtPackManifest::parse(PILOT).expect("reads");
        assert_eq!(
            m.model(DataId(0x0200_0258)).expect("tree").category,
            Category::Plant
        );
        assert_eq!(
            m.model(DataId(0x0100_0A7E)).expect("rock").category,
            Category::Rock
        );
        let far = PILOT.replacen(
            "file = \"tree/lod2.gltf\"",
            "file = \"tree/far.gltf\"\nviews = 16",
            1,
        );
        let m = ArtPackManifest::parse(&far).expect("reads");
        assert_eq!(
            m.model(DataId(0x0200_0258)).expect("tree").category,
            Category::Tree
        );
        let named = PILOT.replacen("foliage = true", "foliage = true\ncategory = \"Water\"", 1);
        let m = ArtPackManifest::parse(&named).expect("reads");
        assert_eq!(
            m.model(DataId(0x0200_0258)).expect("tree").category,
            Category::Water
        );
        let wrong = PILOT.replacen("foliage = true", "foliage = true\ncategory = \"cloud\"", 1);
        let e = ArtPackManifest::parse(&wrong).expect_err("refused");
        assert!(e.0.contains("category"), "{e}");
    }

    /// The `[lighting]` section is kept as the text of a look file, its kinds of day as tables
    /// of their own; a pack without one has none, and one that is not a table is refused.
    #[test]
    fn the_lighting_section_is_kept_as_a_look_files_text() {
        assert_eq!(ArtPackManifest::parse(PILOT).expect("reads").lighting, None);
        let lit = format!(
            "{PILOT}\n[lighting]\nsun = 1.1\ntint = 0.8\n\n[lighting.rainy]\nfog_density = 1.5\n"
        );
        let m = ArtPackManifest::parse(&lit).expect("reads");
        let text = m.lighting.expect("a lighting section");
        let back: toml::Table = text.parse().expect("TOML");
        assert_eq!(back.get("sun").and_then(toml::Value::as_float), Some(1.1));
        assert_eq!(back.get("tint").and_then(toml::Value::as_float), Some(0.8));
        let rainy = back
            .get("rainy")
            .and_then(toml::Value::as_table)
            .expect("[rainy]");
        assert_eq!(
            rainy.get("fog_density").and_then(toml::Value::as_float),
            Some(1.5)
        );
        assert!(back.get("pack").is_none() && back.get("model").is_none());
        let wrong = format!("lighting = 3\n{PILOT}");
        assert!(ArtPackManifest::parse(&wrong).is_err());
    }

    #[test]
    fn a_packs_listing_keys_are_read_when_named_and_refused_when_malformed() {
        let head = "[pack]\nname = \"Houses\"\nformat = 1\n";
        let plain = ArtPackManifest::parse(head).expect("reads");
        assert_eq!(plain.about, About::default(), "none of them is needed");
        let named = ArtPackManifest::parse(&format!(
            "{head}id = \"com.example.houses\"\nversion = \"0.10.2\"\nauthors = \"Ann\"\n\
             license = \"CC0-1.0\"\nmade_for = [\"1999\"]\ndescription = \" Roofs. \"\n"
        ))
        .expect("reads");
        assert_eq!(named.about.id, "com.example.houses");
        assert_eq!(named.about.version, "0.10.2");
        assert_eq!(named.about.authors, ["Ann"]);
        assert_eq!(named.about.license, "CC0-1.0");
        assert_eq!(named.about.made_for, ["1999"]);
        assert_eq!(named.about.description, "Roofs.");
        for bad in [
            "id = \"Houses\"",
            "id = \"houses\"",
            "id = \"com..houses\"",
            "version = \"1.0\"",
            "version = \"01.0.0\"",
            "made_for = [\"2004\"]",
            "authors = 3",
        ] {
            assert!(
                ArtPackManifest::parse(&format!("{head}{bad}\n")).is_err(),
                "{bad}"
            );
        }
    }

    const COVER: &str = "[pack]\nname = \"Meadow\"\nformat = 1\n\
        [[cover]]\nground = [1, 9]\ndensity = 4\nscale = [0.8, 1.25]\nmax_slope = 0.5\n\
        foliage = true\nauthor = \"A\"\ntool = \"T\"\nlicense = \"CC0-1.0\"\n\
        [[cover.lod]]\nfile = \"cover/lupins.gltf\"\ndistance = 20\n\
        [[cover.lod]]\nfile = \"cover/lupins-far.gltf\"\ndistance = 40.0\n\
        [[cover]]\nground = 21\ndensity = 1.5\nauthor = \"A\"\ntool = \"T\"\nlicense = \"CC0-1.0\"\n\
        [[cover.lod]]\nfile = \"cover/fern.gltf\"\ndistance = 30\n";

    #[test]
    fn a_cover_grows_its_model_on_its_terrain_types_as_thickly_and_as_far_as_it_says() {
        let m = ArtPackManifest::parse(COVER).expect("the covers read");
        assert_eq!(m.covers.len(), 2);
        let lupins = &m.covers[0];
        assert_eq!(lupins.grounds, [1, 9]);
        assert_eq!(lupins.density, 4.0);
        assert_eq!(lupins.scale, (0.8, 1.25));
        assert_eq!(lupins.max_slope, 0.5);
        assert_eq!(lupins.reach, 40.0, "its last level's distance");
        assert_eq!(lupins.id, cover_id("cover/lupins.gltf"));
        assert!(is_cover(lupins.id));
        let model = m
            .model(lupins.id)
            .expect("its model is listed with the models");
        assert!(model.foliage);
        assert_eq!(
            model.category,
            Category::Plant,
            "foliage is taken for a plant"
        );
        assert_eq!(model.lods.len(), 2);
        let fern = &m.covers[1];
        assert_eq!(fern.grounds, [21]);
        assert_eq!(
            (fern.scale, fern.max_slope),
            ((1.0, 1.0), 0.6),
            "the defaults"
        );
        assert_ne!(fern.id, lupins.id);
        assert!(!is_cover(DataId(0x0200_0258)));
    }

    /// A cover names no retail record: a `fingerprint` or `draws_on` on it is read past.
    #[test]
    fn a_cover_reads_past_a_fingerprint_and_draws_on() {
        let marked = COVER.replacen(
            "[[cover]]\n",
            "[[cover]]\nfingerprint = \"model 0123456789abcdef\"\ndraws_on = \"none\"\n",
            1,
        );
        assert_ne!(marked, COVER);
        let m = ArtPackManifest::parse(&marked).expect("the covers read");
        let plain = ArtPackManifest::parse(COVER).expect("the covers read");
        assert_eq!(m.covers, plain.covers);
        assert_eq!(m.models, plain.models);
        let lupins = m.model(m.covers[0].id).expect("its model");
        assert!(lupins.fingerprint.is_none());
        assert_eq!(lupins.draws_on, DrawsOn::Matching);
    }

    #[test]
    fn a_cover_that_is_incomplete_or_names_a_retail_object_or_the_road_is_refused() {
        for (from, to) in [
            ("ground = [1, 9]\n", ""),
            ("ground = [1, 9]", "ground = \"road\""),
            ("ground = [1, 9]", "ground = [32]"),
            ("density = 4\n", ""),
            ("density = 4", "density = 0"),
            ("scale = [0.8, 1.25]", "scale = [1.25, 0.8]"),
            ("scale = [0.8, 1.25]", "scale = 1"),
            ("max_slope = 0.5", "max_slope = -1"),
            ("distance = 40.0\n", ""),
            (
                "[[cover]]\nground = [1, 9]",
                "[[cover]]\nid = \"0x02000258\"\nground = [1, 9]",
            ),
            ("license = \"CC0-1.0\"\n[[cover.lod]]", "[[cover.lod]]"),
            ("file = \"cover/fern.gltf\"", "file = \"cover/lupins.gltf\""),
            ("file = \"cover/fern.gltf\"", "file = \"../fern.gltf\""),
        ] {
            assert!(COVER.contains(from), "{from}");
            assert!(
                ArtPackManifest::parse(&COVER.replacen(from, to, 1)).is_err(),
                "{from:?} -> {to:?}"
            );
        }
    }

    #[test]
    fn covers_of_one_model_share_it_each_on_its_own_types() {
        let shared = format!(
            "{COVER}[[cover]]\nground = 15\ndensity = 0.5\nauthor = \"A\"\ntool = \"T\"\n\
             license = \"CC0-1.0\"\n[[cover.lod]]\nfile = \"cover/fern.gltf\"\ndistance = 30\n"
        );
        let m = ArtPackManifest::parse(&shared).expect("the shared cover reads");
        assert_eq!(m.covers.len(), 3);
        assert_eq!(m.covers[1].id, m.covers[2].id);
        assert_eq!((m.covers[1].density, m.covers[2].density), (1.5, 0.5));
        assert_eq!(m.covers[2].grounds, [15]);
        assert_eq!(
            m.models.len(),
            2,
            "one model for the two covers of the fern"
        );
        let other = shared.replacen("distance = 30\n", "distance = 31\n", 2);
        assert!(
            ArtPackManifest::parse(&other.replacen("distance = 31", "distance = 30", 1)).is_err()
        );
    }

    #[test]
    fn a_covers_id_is_its_first_files_whatever_else_the_manifest_holds() {
        let one = ArtPackManifest::parse(COVER).expect("reads");
        let moved = ArtPackManifest::parse(&COVER.replacen("density = 4", "density = 9", 1))
            .expect("reads");
        assert_eq!(one.covers[0].id, moved.covers[0].id);
        assert_eq!(cover_id("cover/lupins.gltf"), cover_id("cover/lupins.gltf"));
        assert_ne!(cover_id("cover/lupins.gltf"), cover_id("cover/lupin.gltf"));
    }

    /// Two materials for one picture, each made from another era's record of it.
    const ERAS: &str = r#"
[pack]
name = "Eras"
format = 1

[[material]]
texture = "0x0500001F"
fingerprint = "picture 64x128 0000f5ffffc30000"
base_color = "materials/eor.png"
author = "a"
tool = "t"
license = "CC0-1.0"

[[material]]
texture = "0x0500001F"
fingerprint = "picture 32x64 008cecf7f7fffff7"
base_color = "materials/2005.png"
author = "a"
tool = "t"
license = "CC0-1.0"
"#;

    /// One picture may have a material for each era's record of it, told apart by fingerprint;
    /// two for the same record, or one for any record before one for a single record, are refused.
    #[test]
    fn one_picture_takes_a_material_for_each_eras_record_of_it() {
        let m = ArtPackManifest::parse(ERAS).expect("a material for each era reads");
        assert_eq!(m.materials.len(), 2);
        assert_eq!(
            m.materials[1].fingerprints,
            [Fingerprint::parse("picture 32x64 008cecf7f7fffff7")]
        );
        assert_eq!(m.materials[0].draws_on, DrawsOn::Matching);
        let same = ERAS.replacen(
            "picture 32x64 008cecf7f7fffff7",
            "picture 64x128 0000f5ffffc30000",
            1,
        );
        let e = ArtPackManifest::parse(&same).expect_err("one record twice");
        assert!(e.0.contains("has a material already"), "{}", e.0);
        let any_first = ERAS.replacen("fingerprint = \"picture 64x128 0000f5ffffc30000\"\n", "", 1);
        let e = ArtPackManifest::parse(&any_first).expect_err("any record, then one");
        assert!(e.0.contains("has a material already"), "{}", e.0);
        let any_last = ERAS.replacen("fingerprint = \"picture 32x64 008cecf7f7fffff7\"\n", "", 1);
        assert!(
            ArtPackManifest::parse(&any_last).is_ok(),
            "one record, then any other"
        );
        for (from, to, why) in [
            (
                "picture 32x64 008cecf7f7fffff7",
                "picture 32x64",
                "is not one",
            ),
            (
                "tool = \"t\"",
                "tool = \"t\"\ndraws_on = \"some\"",
                "draws_on",
            ),
            (
                "tool = \"t\"",
                "tool = \"t\"\npalettes = [\"0x0500001F\"]",
                "palette record",
            ),
        ] {
            let e = ArtPackManifest::parse(&ERAS.replacen(from, to, 1)).expect_err(why);
            assert!(e.0.contains(why), "{why}: {}", e.0);
        }
    }

    /// Two materials for one record told apart by palette both read; one named for both palettes
    /// does not.
    #[test]
    fn one_record_takes_a_material_for_each_palette_it_is_drawn_in() {
        let both = ERAS.replacen(
            "picture 32x64 008cecf7f7fffff7",
            "picture 64x128 0000f5ffffc30000",
            1,
        );
        let apart = both
            .replacen(
                "tool = \"t\"",
                "tool = \"t\"\npalettes = [\"0x04000BA1\"]",
                1,
            )
            .replacen(
                "tool = \"t\"\nlicense",
                "tool = \"t\"\npalettes = [\"0x04000BA5\"]\nlicense",
                2,
            );
        let m = ArtPackManifest::parse(&apart).expect("two palettes apart read");
        assert_eq!(m.materials[0].palettes, [DataId(0x0400_0BA1)]);
        let shared = apart.replacen("[\"0x04000BA5\"]", "[\"0x04000BA1\"]", 1);
        assert!(
            ArtPackManifest::parse(&shared).is_err(),
            "one palette twice"
        );
    }

    /// A model and a ground name the record they were made from in the same way; `draws_on`
    /// reads matching, any or none; a pack may say it was made for the 2005 files.
    #[test]
    fn models_and_grounds_name_their_records_and_a_pack_may_be_made_for_2005() {
        let eor = "fingerprint = \"model 0123456789abcdef\"\n";
        let two = PILOT.replacen("foliage = true\n", &format!("foliage = true\n{eor}"), 1)
            + "\n[[model]]\nid = \"0x02000258\"\nfingerprint = \"model fedcba9876543210\"\n\
               draws_on = \"any\"\nauthor = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n\n\
               [[model.lod]]\nfile = \"old.gltf\"\n";
        let m = ArtPackManifest::parse(&two).expect("a model for each era reads");
        let trees: Vec<_> = m
            .models
            .iter()
            .filter(|t| t.id == DataId(0x0200_0258))
            .collect();
        assert_eq!(trees.len(), 2);
        assert_eq!(trees[1].draws_on, DrawsOn::Any);
        let made_for = PILOT.replacen("format = 1", "format = 1\nmade_for = [\"2005\"]", 1);
        let m = ArtPackManifest::parse(&made_for).expect("made for 2005 reads");
        assert_eq!(m.about.made_for, ["2005"]);
        assert!(!DrawsOn::Nothing.draws(None, None));
        let a = Fingerprint::parse("model 0123456789abcdef");
        let b = Fingerprint::parse("model fedcba9876543210");
        assert!(DrawsOn::Matching.draws(a.as_ref(), a.as_ref()));
        assert!(!DrawsOn::Matching.draws(a.as_ref(), b.as_ref()));
        assert!(
            DrawsOn::Matching.draws(a.as_ref(), None),
            "an unknown record is drawn on"
        );
        assert!(DrawsOn::Any.draws(a.as_ref(), b.as_ref()));
    }
}
