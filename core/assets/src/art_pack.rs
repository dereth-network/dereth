//! An art pack's manifest: which retail objects the pack has models for, the files of each
//! model's levels of detail and the distances they are drawn to, and who made each model, with
//! what, under which license.
//!
//! A pack is a folder for now: `manifest.toml` at its top, beside the glTF 2.0 models it names.
//! Every file is named by a path relative to the folder, with `/` between its parts and nothing
//! that climbs out of the folder, so the manifest says nothing about where the folder is. That
//! is what lets a pack be packed later into one data file: each file the manifest names becomes
//! a record keyed by its path, and the manifest itself one more, read through the same paths.
//!
//! ```toml
//! [pack]
//! name = "Remastered"
//! format = 1
//!
//! [[model]]
//! id = "0x02000258"          # a setup (0x02...) or a graphics object (0x01...)
//! foliage = true             # its cut-out materials are growth: two-sided, lit through
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
    /// Its models, in the order the manifest lists them.
    pub models: Vec<ManifestModel>,
    /// Its ground materials, in the order the manifest lists them.
    pub grounds: Vec<ManifestGround>,
    /// Its materials for retail pictures, in the order the manifest lists them.
    pub materials: Vec<ManifestMaterial>,
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
    /// Who made it, with what, and under which license.
    pub credit: Credit,
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
        Ok(Self {
            name,
            models,
            grounds,
            materials,
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
        return fail("no [[model.lod]] level of detail");
    };
    if levels.is_empty() {
        return fail("no [[model.lod]] level of detail");
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
    Ok(ManifestModel {
        id,
        lods,
        foliage,
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
}
