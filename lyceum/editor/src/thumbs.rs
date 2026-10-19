//! The model browser's pictures: each model drawn off screen on a device of the editor's own,
//! kept as a PNG in a cache folder, and handed to the panels as a texture.
//!
//! A picture is the model as the client draws it in a preview: its parts at their resting
//! placement, lit by one light, seen from the south-west and a little above, framed to its
//! bounding box, on black. It is drawn [`SIZE`] * 2 pixels square and halved, so its edges are
//! smooth. The cache folder holds one folder per set of files (named by the portal file's
//! fingerprint), so pictures of other files are never shown.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, TextureData, TextureFormat, Vec3};
use dereth_render::camera::Viewport;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::preview::PreviewSpace;

/// A picture's side, in pixels.
pub const SIZE: u32 = 48;

/// The camera's heading (degrees, clockwise from north) and pitch.
const YAW: f32 = 35.0;
const PITCH: f32 = -24.0;

/// The pictures of one set of files.
pub struct Thumbs {
    gpu: Gpu,
    space: PreviewSpace,
    store: Arc<RetailDatStore>,
    dir: PathBuf,
    ready: BTreeMap<u32, TextureData>,
    failed: BTreeSet<u32>,
    /// How many pictures were drawn, and how many read from the cache.
    pub drawn: u64,
    pub read: u64,
}

impl std::fmt::Debug for Thumbs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumbs")
            .field("dir", &self.dir)
            .field("ready", &self.ready.len())
            .field("failed", &self.failed.len())
            .finish_non_exhaustive()
    }
}

/// The folder the pictures of `store`'s files are kept in, under `root`.
#[must_use]
pub fn folder(root: &Path, store: &RetailDatStore) -> PathBuf {
    let fp = dereth_dat::overlay::fingerprint(store.portal());
    let name: String = fp[..8].iter().map(|b| format!("{b:02x}")).collect();
    root.join(name)
}

/// The cache folder the editor keeps its pictures in when it is not told one: the user's cache
/// folder, else the system's temporary folder.
#[must_use]
pub fn default_root() -> PathBuf {
    dereth_client_runtime::platform::files::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("editor-thumbnails")
}

impl Thumbs {
    /// The pictures of `store`'s files, kept under `root`, drawn on a device of `backend` of the
    /// machine's own graphics card.
    ///
    /// # Errors
    /// The device will not open.
    pub fn open(store: Arc<RetailDatStore>, backend: Backend, root: &Path) -> Result<Self, String> {
        let cfg = DeviceConfig {
            width: SIZE * 2,
            height: SIZE * 2,
            force_software: false,
            ..DeviceConfig::default()
        };
        let gpu = Gpu::new_on(backend, None, &cfg).map_err(|e| format!("pictures: {e}"))?;
        let assets = Arc::new(dereth_world_data::anim_assets::DatAnimAssets::new(
            Arc::clone(&store),
        ));
        let mut space = PreviewSpace::new(assets);
        space.set_light(
            dereth_world_render::lighting::LightType::Directional,
            2.0,
            Vec3::new(0.3, 1.9, 0.65),
        );
        space.mode.set_fov(0.6);
        Ok(Self {
            gpu,
            space,
            dir: folder(root, &store),
            store,
            ready: BTreeMap::new(),
            failed: BTreeSet::new(),
            drawn: 0,
            read: 0,
        })
    }

    /// Where model `id`'s picture is kept.
    #[must_use]
    pub fn path(&self, id: DataId) -> PathBuf {
        self.dir.join(format!("{:08X}.png", id.raw()))
    }

    /// Model `id`'s picture: held, else read from the cache, else drawn when `budget` allows
    /// (and taken from it). `None` for one that will not draw, or that waits for a budget.
    pub fn get(&mut self, id: DataId, budget: &mut u32) -> Option<&TextureData> {
        let raw = id.raw();
        if self.failed.contains(&raw) {
            return None;
        }
        if !self.ready.contains_key(&raw) {
            let picture = match read_png(&self.path(id)) {
                Some(p) => {
                    self.read += 1;
                    p
                }
                None if *budget > 0 => {
                    *budget -= 1;
                    match self.draw(id) {
                        Ok(Some(p)) => {
                            self.drawn += 1;
                            let _ = write_png(&self.path(id), &p);
                            p
                        }
                        Ok(None) | Err(_) => {
                            self.failed.insert(raw);
                            return None;
                        }
                    }
                }
                None => return None,
            };
            self.ready.insert(raw, picture);
        }
        self.ready.get(&raw)
    }

    /// Draw model `id` and read the picture back. `None` for a model that will not load.
    fn draw(&mut self, id: DataId) -> Result<Option<TextureData>, String> {
        self.space.remove_all_objects();
        let Some(i) = self
            .space
            .add_model(&self.store, &mut self.gpu, id)
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let bb = self
            .space
            .object(i)
            .map(|o| o.bounding_box(&self.store))
            .ok_or("no object")?;
        let center = Vec3::new(
            (bb.min.x + bb.max.x) * 0.5,
            (bb.min.y + bb.max.y) * 0.5,
            (bb.min.z + bb.max.z) * 0.5,
        );
        let radius = Vec3::new(
            bb.max.x - bb.min.x,
            bb.max.y - bb.min.y,
            bb.max.z - bb.min.z,
        )
        .magnitude()
        .max(0.1)
            * 0.5;
        let fov = self.space.mode.fov_radians;
        let distance = radius / dereth_primitives::num::math::sinf(fov * 0.5) * 1.05;
        let look = dereth_client_runtime::camera::FreeCamera::new(
            Vec3::ZERO,
            YAW.to_radians(),
            PITCH.to_radians(),
        );
        let f = look.forward();
        let eye = Vec3::new(
            center.x - f.x * distance,
            center.y - f.y * distance,
            center.z - f.z * distance,
        );
        self.space.mode.view_frame = dereth_client_runtime::camera::FreeCamera::new(
            eye,
            YAW.to_radians(),
            PITCH.to_radians(),
        )
        .frame();
        let side = SIZE * 2;
        self.gpu.begin_frame().map_err(|e| e.to_string())?;
        let drawn = self.space.draw(
            &mut self.gpu,
            Viewport {
                x: 0,
                y: 0,
                width: side,
                height: side,
            },
            None,
            fov,
        );
        self.gpu.end_frame().map_err(|e| e.to_string())?;
        drawn.map_err(|e| e.to_string())?;
        let image = self.gpu.capture().map_err(|e| e.to_string())?;
        Ok(Some(halve(&image.bgra, image.width, image.height)))
    }
}

/// `bgra` (`w` by `h`) at half its size, each pixel the mean of four, opaque.
fn halve(bgra: &[u8], w: u32, h: u32) -> TextureData {
    let (hw, hh) = (w / 2, h / 2);
    let mut out = Vec::with_capacity((hw * hh * 4) as usize);
    for y in 0..hh {
        for x in 0..hw {
            let mut sum = [0u32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let i = (((y * 2 + dy) * w + x * 2 + dx) * 4) as usize;
                for (c, s) in sum.iter_mut().enumerate() {
                    *s += u32::from(bgra.get(i + c).copied().unwrap_or(0));
                }
            }
            for s in sum {
                out.push(u8::try_from(s / 4).unwrap_or(255));
            }
            out.push(255);
        }
    }
    TextureData {
        width: hw,
        height: hh,
        format: TextureFormat::Bgra8,
        levels: vec![out],
    }
}

/// A cached picture, as a texture.
fn read_png(path: &Path) -> Option<TextureData> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    for px in buf.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Some(TextureData {
        width: info.width,
        height: info.height,
        format: TextureFormat::Bgra8,
        levels: vec![buf],
    })
}

/// A picture, kept in the cache as a PNG.
fn write_png(path: &Path, t: &TextureData) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut rgba = t.levels.first().cloned().unwrap_or_default();
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let file = std::fs::File::create(path)?;
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), t.width, t.height);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let mut w = e.write_header().map_err(std::io::Error::other)?;
    w.write_image_data(&rgba).map_err(std::io::Error::other)?;
    Ok(())
}
