//! The pictures frames are drawn into, kept between frames: each is cleared as a frame starts
//! drawing in it, so one frame's picture can be the next's, and none has to be made anew. A frame
//! may hold several of one size (the world and portal space); those it hands out stay its own
//! until it is shown.

use std::collections::HashMap;

/// How many frames a picture no frame has drawn into is kept: long enough for a panel that comes
/// and goes, and not so long that a window's old sizes pile up.
const KEEP_FRAMES: u64 = 120;

/// What a drawing target is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TargetKey {
    pub(crate) label: &'static str,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) samples: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) usage: wgpu::TextureUsages,
}

struct Kept {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Handed out in this frame.
    taken: bool,
    used: u64,
}

/// The pictures kept.
#[derive(Default)]
pub(crate) struct Targets {
    kept: HashMap<TargetKey, Vec<Kept>>,
    frame: u64,
}

impl Targets {
    /// A picture of this kind not yet taken in this frame; whether it had to be made.
    pub(crate) fn take(
        &mut self,
        device: &wgpu::Device,
        key: TargetKey,
    ) -> (wgpu::Texture, wgpu::TextureView, bool) {
        let frame = self.frame;
        let list = self.kept.entry(key).or_default();
        if let Some(k) = list.iter_mut().find(|k| !k.taken) {
            k.taken = true;
            k.used = frame;
            return (k.texture.clone(), k.view.clone(), false);
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(key.label),
            size: wgpu::Extent3d {
                width: key.width,
                height: key.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: key.samples,
            dimension: wgpu::TextureDimension::D2,
            format: key.format,
            usage: key.usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        list.push(Kept {
            texture: texture.clone(),
            view: view.clone(),
            taken: true,
            used: frame,
        });
        (texture, view, true)
    }

    /// A frame is shown: its pictures are free again, and those long out of use are let go.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        for list in self.kept.values_mut() {
            list.retain_mut(|k| {
                k.taken = false;
                frame.saturating_sub(k.used) < KEEP_FRAMES
            });
        }
        self.kept.retain(|_, list| !list.is_empty());
        self.frame += 1;
    }
}
