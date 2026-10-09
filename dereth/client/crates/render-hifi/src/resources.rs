//! The named GPU resources the passes share, created on first use and held to a video memory
//! budget. A resource no pass has asked for in [`IDLE_FRAMES`] frames is released.

use std::collections::HashMap;

use crate::HifiError;

/// The video memory budget when none is set: 4 GiB.
pub const DEFAULT_BUDGET: u64 = 4 << 30;

/// How many frames a resource is kept after the last frame that asked for it.
pub const IDLE_FRAMES: u64 = 120;

/// The resources passes share, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceName {
    /// The world replayed with its ordinary pipelines, in the target's format.
    WorldColour,
    /// The world's depth.
    WorldDepth,
    /// The world's depth as it stood before an indoor step cleared it.
    WorldDepthBeforeClear,
    /// The world re-shaded in linear high dynamic range.
    SceneHdr,
    /// The world's view-space normals, with the material class in alpha.
    ViewNormal,
    /// The landscape's heights over the resident window.
    TerrainHeight,
    /// The landscape's smooth normals.
    TerrainNormal,
    /// The landscape's surface classes: water, road, grass, rock, snow.
    TerrainClass,
    /// The sun's shadow cascades.
    SunShadow,
    /// The sky's transmittance table.
    TransmittanceLut,
    /// The sky's view table.
    SkyViewLut,
    /// The ambient occlusion term.
    Ao,
    /// The bent normals from the occlusion search.
    BentNormal,
    /// What each relit pixel reflects of new light, faded by the fog: the bounced light's
    /// surface colour.
    GiSurface,
    /// A second picture in the target's format, for passes that read one picture and write
    /// another.
    ScratchColour,
    /// A copy of the re-shaded picture, for passes that read it while they draw into it.
    ScratchHdr,
    /// A copy of the normals and classes as the opaque span left them, for the pass that
    /// rewrites the landscape's.
    ScratchNormal,
}

/// One shared resource.
#[derive(Debug)]
pub enum Resource {
    /// A texture and its default view.
    Texture(wgpu::Texture, wgpu::TextureView),
    /// A buffer.
    Buffer(wgpu::Buffer),
}

/// What a two-dimensional texture resource is: enough to tell whether the one held still fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureSpec {
    /// Width, texels.
    pub width: u32,
    /// Height, texels.
    pub height: u32,
    /// The texel format.
    pub format: wgpu::TextureFormat,
    /// What it is used for.
    pub usage: wgpu::TextureUsages,
}

impl TextureSpec {
    /// The bytes the texture takes, one level.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        let texel = self
            .format
            .block_copy_size(Some(wgpu::TextureAspect::All))
            .or_else(|| {
                self.format
                    .block_copy_size(Some(wgpu::TextureAspect::DepthOnly))
            })
            .unwrap_or(8);
        u64::from(self.width) * u64::from(self.height) * u64::from(texel)
    }
}

/// The accounting of the shared resources, apart from the resources themselves: what each name
/// is charged against the budget, and the last frame that asked for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    budget: u64,
    used: u64,
    frame: u64,
    charges: HashMap<ResourceName, (u64, u64)>,
}

impl Ledger {
    /// Nothing charged, with `budget` bytes to spend.
    #[must_use]
    pub fn new(budget: u64) -> Self {
        Self {
            budget,
            ..Self::default()
        }
    }

    /// The bytes charged.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.used
    }

    /// The bytes still free under the budget.
    #[must_use]
    pub fn left(&self) -> u64 {
        self.budget.saturating_sub(self.used)
    }

    /// Check that `bytes` more would stay inside the budget.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when it would not.
    pub fn check(&self, bytes: u64) -> Result<(), HifiError> {
        if bytes > self.left() {
            return Err(HifiError::Budget {
                wanted: bytes,
                left: self.left(),
            });
        }
        Ok(())
    }

    /// Charge `name` with `bytes`, in place of what it was charged before.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when that would pass the budget; nothing changes then.
    pub fn charge(&mut self, name: ResourceName, bytes: u64) -> Result<(), HifiError> {
        let before = self.charges.get(&name).map_or(0, |c| c.0);
        let left = self.budget.saturating_sub(self.used - before);
        if bytes > left {
            return Err(HifiError::Budget {
                wanted: bytes,
                left,
            });
        }
        self.used = self.used - before + bytes;
        self.charges.insert(name, (bytes, self.frame));
        Ok(())
    }

    /// Note that this frame asked for `name`; whether it is charged.
    pub fn touch(&mut self, name: ResourceName) -> bool {
        let frame = self.frame;
        self.charges.get_mut(&name).map(|c| c.1 = frame).is_some()
    }

    /// Drop the charge of `name`.
    pub fn release(&mut self, name: ResourceName) {
        if let Some((bytes, _)) = self.charges.remove(&name) {
            self.used -= bytes;
        }
    }

    /// Drop every charge.
    pub fn release_all(&mut self) {
        self.charges.clear();
        self.used = 0;
    }

    /// Start a new frame. The names no frame has asked for in [`IDLE_FRAMES`] lose their charges,
    /// and are returned.
    pub fn begin_frame(&mut self) -> Vec<ResourceName> {
        self.frame += 1;
        let frame = self.frame;
        let idle: Vec<ResourceName> = self
            .charges
            .iter()
            .filter(|(_, (_, used))| frame - used > IDLE_FRAMES)
            .map(|(n, _)| *n)
            .collect();
        for n in &idle {
            self.release(*n);
        }
        idle
    }

    /// Whether `name` is charged.
    #[must_use]
    pub fn holds(&self, name: ResourceName) -> bool {
        self.charges.contains_key(&name)
    }
}

/// One held resource, and its texture spec when it is a texture.
#[derive(Debug)]
struct Held {
    resource: Resource,
    spec: Option<TextureSpec>,
}

/// The resources passes share, and their accounting.
#[derive(Debug)]
pub struct Resources {
    ledger: Ledger,
    held: HashMap<ResourceName, Held>,
}

impl Resources {
    /// No resources, with `budget` bytes of video memory to spend.
    #[must_use]
    pub fn new(budget: u64) -> Self {
        Self {
            ledger: Ledger::new(budget),
            held: HashMap::new(),
        }
    }

    /// The bytes held.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.ledger.used()
    }

    /// The bytes still free under the budget.
    #[must_use]
    pub fn left(&self) -> u64 {
        self.ledger.left()
    }

    /// Check that `bytes` more would stay inside the budget.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when it would not.
    pub fn check(&self, bytes: u64) -> Result<(), HifiError> {
        self.ledger.check(bytes)
    }

    /// Hold `resource` under `name`, charged at `bytes`, replacing what was held there.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when it would pass the budget; nothing is held then.
    pub fn insert(
        &mut self,
        name: ResourceName,
        resource: Resource,
        bytes: u64,
    ) -> Result<(), HifiError> {
        self.insert_held(name, resource, bytes, None)
    }

    fn insert_held(
        &mut self,
        name: ResourceName,
        resource: Resource,
        bytes: u64,
        spec: Option<TextureSpec>,
    ) -> Result<(), HifiError> {
        self.ledger.charge(name, bytes)?;
        self.held.insert(name, Held { resource, spec });
        Ok(())
    }

    /// The texture held under `name` if it is `spec`, else a new one made to it and held in its
    /// place; its default view.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when a new one would pass the budget.
    pub fn texture(
        &mut self,
        device: &wgpu::Device,
        name: ResourceName,
        spec: TextureSpec,
    ) -> Result<wgpu::TextureView, HifiError> {
        if let Some(h) = self.held.get(&name) {
            if h.spec == Some(spec) {
                if let Resource::Texture(_, view) = &h.resource {
                    let view = view.clone();
                    self.ledger.touch(name);
                    return Ok(view);
                }
            }
        }
        // What is held under the name is released before its replacement is charged.
        self.release(name);
        self.check(spec.bytes())?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("high-fidelity"),
            size: wgpu::Extent3d {
                width: spec.width.max(1),
                height: spec.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: spec.format,
            usage: spec.usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.insert_held(
            name,
            Resource::Texture(texture, view.clone()),
            spec.bytes(),
            Some(spec),
        )?;
        Ok(view)
    }

    /// The resource held under `name`, noting that this frame asked for it.
    pub fn touch(&mut self, name: ResourceName) -> Option<&Resource> {
        self.ledger.touch(name);
        self.held.get(&name).map(|h| &h.resource)
    }

    /// The resource held under `name`.
    #[must_use]
    pub fn get(&self, name: ResourceName) -> Option<&Resource> {
        self.held.get(&name).map(|h| &h.resource)
    }

    /// Release the resource held under `name`.
    pub fn release(&mut self, name: ResourceName) {
        self.held.remove(&name);
        self.ledger.release(name);
    }

    /// Release everything.
    pub fn release_all(&mut self) {
        self.held.clear();
        self.ledger.release_all();
    }

    /// Start a new frame, releasing every resource no frame has asked for in [`IDLE_FRAMES`].
    pub fn begin_frame(&mut self) {
        for name in self.ledger.begin_frame() {
            self.held.remove(&name);
        }
    }

    /// How many resources are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.held.len()
    }

    /// Whether none is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}
