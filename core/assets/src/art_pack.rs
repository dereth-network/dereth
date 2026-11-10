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
//! author = "..."
//! tool = "..."
//! license = "..."
//! ```
//!
//! Only `base_color`, `size` and the credit are required. The pictures of every material are
//! square and repeat seamlessly, seen from straight above with north up.
//!
//! A `[[steep]]` material, with the same keys, is laid on the steep slopes of the terrain types
//! it names, in place of the scree and rock the presentation lays on steep ground of a type with
//! none: an obsidian cliff, a sandstone face. A type may have one of each kind.
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
//! flowers the presentation grows itself through its `[lighting]`'s grass tables.
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
//! normal_scale = 1.0         # how strongly the normal picture bends the normal
//! author = "..."
//! tool = "..."
//! license = "..."
//! ```
//!
//! Only `texture`, `base_color` and the credit are required.
//!
//! **A model's category** says which of a player's switches turns it on: `tree`, `plant`, `rock`
//! or `water` (growth in and by the water: lily pads, reeds). A model that names none is a tree
//! when it has a level of pictures round it, a plant when it is foliage, and a rock otherwise; a
//! model is a water model only when it says so.
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
//! the data the pack's ids are chosen from, `end-of-retail` or `1999` (a pack only replaces
//! things by their ids, so it draws on any world whose data has them).
//!
//! This module reads the manifest only. The models and their pictures are read by whatever
//! draws them.

use dereth_primitives::DataId;

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
pub const MADE_FOR: [&str; 2] = ["end-of-retail", "1999"];

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
    /// How strongly the normal picture bends the normal.
    pub normal_scale: f32,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
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
    /// Laid on the steep slopes of its grounds rather than on their flat ground: a `[[steep]]`
    /// table's.
    pub steep: bool,
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
            if models.iter().any(|o| o.id == model.id) {
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
                for k in &ground.grounds {
                    if grounds
                        .iter()
                        .any(|o| o.steep == steep && o.grounds.contains(k))
                    {
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
        let mut materials: Vec<ManifestMaterial> = Vec::with_capacity(listed.len());
        for (i, m) in listed.iter().enumerate() {
            let Some(m) = m.as_table() else {
                return fail(format!("material {} is not a table", i + 1));
            };
            let material =
                material(m).map_err(|e| ManifestError(format!("material {}: {}", i + 1, e.0)))?;
            for t in &material.textures {
                if materials.iter().any(|o| o.textures.contains(t)) {
                    return fail(format!(
                        "material {}: {:#010X} has a material already",
                        i + 1,
                        t.0
                    ));
                }
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
            lighting,
        })
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
    Ok(ManifestGround {
        grounds,
        base_color,
        normal: picture("normal")?,
        height: picture("height")?,
        roughness: picture("roughness")?,
        occlusion: picture("occlusion")?,
        size,
        normal_scale,
        steep: false,
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

fn material(t: &toml::Table) -> Result<ManifestMaterial, ManifestError> {
    let textures = textures_of(t.get("texture"))?;
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
        normal_scale,
        credit: Credit {
            author: text(t, "author")?.to_owned(),
            tool: text(t, "tool")?.to_owned(),
            license: text(t, "license")?.to_owned(),
        },
    })
}

fn model(t: &toml::Table) -> Result<ManifestModel, ManifestError> {
    let id = object_id(t.get("id").and_then(toml::Value::as_str).unwrap_or(""))?;
    shape(t, id, "model")
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
        let road = m.ground(Ground::Road).expect("the road's material");
        assert!((road.size - 8.5).abs() < 1e-6);
        assert!((road.normal_scale - 0.5).abs() < 1e-6);
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
            "made_for = [\"2005\"]",
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
}
