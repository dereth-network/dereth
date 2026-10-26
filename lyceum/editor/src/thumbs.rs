//! The model browser's pictures: each model drawn off screen on a device of the editor's own,
//! kept as a PNG in a cache folder, and handed to the panels as a texture.
//!
//! A picture is the model as the client draws it in a preview: its parts at their resting
//! placement, lit by one light, seen from the south-west and a little above, framed to its
//! bounding box, on black. It is drawn [`SIZE`] * 2 pixels square and halved, so its edges are
//! smooth. The cache folder holds one folder per set of files (named by the portal file's
//! fingerprint), so pictures of other files are never shown.
//!
//! The pictures are read and drawn on a thread of their own ([`Worker`]), on its own device, so
//! the editor's frame never waits on one: a picture asked for is shown in a later frame.
//!
//! The worker also draws a setup dressed in an object description, as the client dresses one a
//! server describes ([`Worker::get_look`]): the weenie editor's clothing pictures. Those are kept
//! by a key the asker gives, which names the setup and the description.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

use dereth_animation::parts::ObjDesc;
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

/// The pictures' field of view, in radians, and their light: its strength and its direction.
pub const FOV: f32 = 0.6;
pub const LIGHT: f32 = 2.0;
pub const LIGHT_DIRECTION: Vec3 = Vec3::new(0.3, 1.9, 0.65);

/// How many pictures are asked of the worker at most before it has answered: a list scrolled
/// past quickly leaves little behind to draw.
const IN_FLIGHT: usize = 16;

/// What draws the pictures of one set of files, on the worker's thread.
struct Thumbs {
    gpu: Gpu,
    space: PreviewSpace,
    store: Arc<RetailDatStore>,
    dir: PathBuf,
}

/// What a picture is of: a model, or a setup dressed in a description (by the asker's key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Model(u32),
    Look(u64),
}

/// A picture asked of the worker.
enum Ask {
    Model(u32),
    Look(u64, DataId, ObjDesc),
}

impl Ask {
    fn key(&self) -> Key {
        match self {
            Self::Model(id) => Key::Model(*id),
            Self::Look(key, ..) => Key::Look(*key),
        }
    }
}

/// A picture the worker answers with: read from the cache, or drawn; `None` for a model that
/// will not draw.
struct Answer {
    key: Key,
    picture: Option<TextureData>,
    drawn: bool,
}

/// The pictures of one set of files, read and drawn on a thread of their own and held here as
/// they come back.
pub struct Worker {
    asks: Option<mpsc::Sender<Ask>>,
    answers: mpsc::Receiver<Result<Answer, String>>,
    thread: Option<std::thread::JoinHandle<()>>,
    dir: PathBuf,
    ready: BTreeMap<Key, TextureData>,
    failed: BTreeSet<Key>,
    asked: BTreeSet<Key>,
    /// Why there are no pictures, when the device would not open.
    pub problem: Option<String>,
    /// How many pictures were drawn, and how many read from the cache.
    pub drawn: u64,
    pub read: u64,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("dir", &self.dir)
            .field("ready", &self.ready.len())
            .field("failed", &self.failed.len())
            .field("asked", &self.asked.len())
            .finish_non_exhaustive()
    }
}

impl Worker {
    /// The pictures of `store`'s files, kept under `root`, drawn on a device of `backend` of the
    /// machine's own graphics card, which the worker opens.
    #[must_use]
    pub fn open(store: Arc<RetailDatStore>, backend: Backend, root: &Path) -> Self {
        let dir = folder(root, &store);
        let (asks, asked) = mpsc::channel::<Ask>();
        let (answer, answers) = mpsc::channel();
        let at = dir.clone();
        let thread = std::thread::Builder::new()
            .name("lyceum pictures".into())
            .spawn(move || {
                let mut thumbs = match Thumbs::open(store, backend, at) {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = answer.send(Err(e));
                        return;
                    }
                };
                for ask in asked {
                    let key = ask.key();
                    let (picture, drawn) = thumbs.picture(ask);
                    if answer
                        .send(Ok(Answer {
                            key,
                            picture,
                            drawn,
                        }))
                        .is_err()
                    {
                        return;
                    }
                }
            })
            .ok();
        Self {
            asks: Some(asks),
            answers,
            thread,
            dir,
            ready: BTreeMap::new(),
            failed: BTreeSet::new(),
            asked: BTreeSet::new(),
            problem: None,
            drawn: 0,
            read: 0,
        }
    }

    /// Where model `id`'s picture is kept.
    #[must_use]
    pub fn path(&self, id: DataId) -> PathBuf {
        self.dir.join(format!("{:08X}.png", id.raw()))
    }

    /// Model `id`'s picture, when it has come back. One not yet asked for is asked for while
    /// `budget` lasts (and taken from it) and the worker is not too far behind.
    pub fn get(&mut self, id: DataId, budget: &mut u32) -> Option<&TextureData> {
        self.fetch(Ask::Model(id.raw()), budget)
    }

    /// The picture of `setup` dressed in `objdesc`, kept by `key` (which the asker makes to name
    /// the two), when it has come back; asked for as [`Self::get`] asks.
    pub fn get_look(
        &mut self,
        key: u64,
        setup: DataId,
        objdesc: &ObjDesc,
        budget: &mut u32,
    ) -> Option<&TextureData> {
        if self.ready.contains_key(&Key::Look(key)) {
            return self.ready.get(&Key::Look(key));
        }
        self.fetch(Ask::Look(key, setup, objdesc.clone()), budget)
    }

    fn fetch(&mut self, ask: Ask, budget: &mut u32) -> Option<&TextureData> {
        self.take_answers();
        let key = ask.key();
        if !self.ready.contains_key(&key)
            && !self.failed.contains(&key)
            && !self.asked.contains(&key)
            && *budget > 0
            && self.asked.len() < IN_FLIGHT
            && self.problem.is_none()
        {
            if let Some(asks) = &self.asks {
                if asks.send(ask).is_ok() {
                    *budget -= 1;
                    self.asked.insert(key);
                }
            }
        }
        self.ready.get(&key)
    }

    /// Wait until every picture asked for has come back.
    pub fn wait(&mut self) {
        while !self.asked.is_empty() && self.problem.is_none() {
            match self.answers.recv() {
                Ok(a) => self.take(a),
                Err(_) => break,
            }
        }
    }

    fn take_answers(&mut self) {
        while let Ok(a) = self.answers.try_recv() {
            self.take(a);
        }
    }

    fn take(&mut self, a: Result<Answer, String>) {
        match a {
            Ok(a) => {
                self.asked.remove(&a.key);
                match a.picture {
                    Some(p) => {
                        if a.drawn {
                            self.drawn += 1;
                        } else {
                            self.read += 1;
                        }
                        self.ready.insert(a.key, p);
                    }
                    None => {
                        self.failed.insert(a.key);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("the model browser shows no pictures: {e}");
                self.problem = Some(e);
                self.asked.clear();
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // The worker ends after the picture it is on once nothing more can be asked of it.
        self.asks = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
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
    /// The pictures of `store`'s files, kept in `dir`, drawn on a device of `backend` of the
    /// machine's own graphics card.
    ///
    /// # Errors
    /// The device will not open.
    fn open(store: Arc<RetailDatStore>, backend: Backend, dir: PathBuf) -> Result<Self, String> {
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
            LIGHT,
            LIGHT_DIRECTION,
        );
        space.mode.set_fov(FOV);
        Ok(Self {
            gpu,
            space,
            dir,
            store,
        })
    }

    /// The picture asked for, read from the cache, else drawn and kept there; and whether it was
    /// drawn. `None` for a model that will not draw.
    fn picture(&mut self, ask: Ask) -> (Option<TextureData>, bool) {
        let path = self.dir.join(match &ask {
            Ask::Model(id) => format!("{id:08X}.png"),
            Ask::Look(key, ..) => format!("look-{key:016X}.png"),
        });
        if let Some(p) = read_png(&path) {
            return (Some(p), false);
        }
        match self.draw(&ask) {
            Ok(Some(p)) => {
                let _ = write_png(&path, &p);
                (Some(p), true)
            }
            Ok(None) | Err(_) => (None, true),
        }
    }

    /// Draw what is asked and read the picture back. `None` for a model that will not load.
    fn draw(&mut self, ask: &Ask) -> Result<Option<TextureData>, String> {
        self.space.remove_all_objects();
        let added = match ask {
            Ask::Model(id) => self
                .space
                .add_model(&self.store, &mut self.gpu, DataId(*id)),
            Ask::Look(_, setup, objdesc) => {
                self.space
                    .add_object_dressed(&self.store, &mut self.gpu, *setup, Some(objdesc))
            }
        };
        let Some(i) = added.map_err(|e| e.to_string())? else {
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
        self.space.mode.view_frame = camera(center, radius, fov, 1.0);
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

/// The camera a picture is seen from: from the south-west and a little above, looking at
/// `center`, far enough that a ball of `radius` about it fills a field of view `fov` high, and
/// then `nearer` times nearer.
#[must_use]
pub fn camera(center: Vec3, radius: f32, fov: f32, nearer: f32) -> dereth_primitives::Frame {
    let distance = radius / dereth_primitives::num::math::sinf(fov * 0.5) * 1.05 / nearer.max(0.01);
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
    dereth_client_runtime::camera::FreeCamera::new(eye, YAW.to_radians(), PITCH.to_radians())
        .frame()
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
