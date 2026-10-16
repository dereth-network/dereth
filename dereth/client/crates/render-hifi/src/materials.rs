//! An art pack's materials drawn in place of retail pictures.
//!
//! The scene names, each frame, the texture slots that hold a retail picture one of the pack's
//! materials stands for ([`HifiFrame::material_slots`]). In a re-shaded frame every draw whose
//! first stage's texture is such a slot is drawn with the material: the pipeline derived from
//! its own for a material ([`Variant::Material`]), the material's base color bound in place of
//! the slot and its other pictures in a group of their own. Everything else about the draw --
//! its geometry and coordinates, its blend, cut-out and depth, its lights -- is the recorded
//! draw's, so the retail surface's flags are kept.
//!
//! The draws of objects an art pack has models for ([`Mark::ArtPackCovered`]) are never drawn
//! with a material: the pack's model stands in for the whole object. The retail look draws no
//! material at all: the materials are drawn only while the art pack is.
//!
//! A frame that is not re-shaded draws the retail pictures, as does a draw whose material
//! pipeline is still being built.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use crate::derive::cache::PipelineCache;
use crate::derive::material::MATERIAL_GROUP;
use crate::derive::{DerivedKey, Variant};
use crate::shared::pack_image;
use crate::snapshot::{HifiMaterials, HifiPackImage};
use crate::{DrawAction, DrawNote, HifiFrame, HifiSettings, Mark, SideTables, SidecarContext};

/// Each draw of `range` drawn with a material, with the material's index, in command order: the
/// draws whose first stage's texture, as the recording binds it at the draw, is one of `slots`,
/// less the draws of objects an art pack has a model for.
#[must_use]
pub fn material_draws(
    tables: &SideTables,
    slots: &[(u32, u32)],
    range: Range<u32>,
) -> Vec<(u32, u32)> {
    if slots.is_empty() {
        return Vec::new();
    }
    let of_slot: HashMap<u32, u32> = slots.iter().copied().collect();
    // The draws of objects an art pack has models for.
    let mut covered: Vec<Range<u32>> = Vec::new();
    let mut open = None;
    for (at, mark) in &tables.marks {
        match mark {
            Mark::ArtPackCovered => open = Some(*at),
            Mark::ArtPackCoveredEnd => {
                if let Some(from) = open.take() {
                    covered.push(from..*at);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    let mut binds = tables.binds.iter().filter(|b| b.group == 1).peekable();
    let mut bound: Option<u32> = None;
    for note in &tables.draws {
        while let Some(b) = binds.next_if(|b| b.cmd < note.cmd) {
            bound = Some(b.slot.0);
        }
        if !range.contains(&note.cmd) || note.splat {
            continue;
        }
        let Some(m) = bound.and_then(|s| of_slot.get(&s)) else {
            continue;
        };
        if covered.iter().any(|r| r.contains(&note.cmd)) {
            continue;
        }
        out.push((note.cmd, *m));
    }
    out
}

/// The derived key a draw is drawn with a material by.
#[must_use]
pub fn material_key(note: &DrawNote) -> DerivedKey {
    DerivedKey {
        key: note.key,
        splat: note.splat,
        variant: Variant::Material,
    }
}

/// Whether a frame drawn with `settings` draws the materials `frame` carries.
#[must_use]
pub fn drawn(settings: &HifiSettings, frame: &HifiFrame) -> bool {
    settings.art_pack
        && frame
            .materials
            .as_ref()
            .is_some_and(|m| !m.materials.is_empty())
}

/// One material on the device: its base color in the recorded texture group's layout, and its
/// other pictures in the material's group.
#[derive(Debug)]
struct GpuMaterial {
    albedo: wgpu::BindGroup,
    maps: wgpu::BindGroup,
}

/// The materials of one pack on the device.
#[derive(Debug)]
struct MaterialsGpu {
    serial: u64,
    materials: Vec<GpuMaterial>,
    _textures: Vec<wgpu::Texture>,
    /// The bytes their pictures take on the device.
    bytes: u64,
}

/// The materials on the device and this frame's draws of them.
#[derive(Debug, Default)]
pub struct Materials {
    gpu: Option<MaterialsGpu>,
    /// The material's group layout and the pipeline layout of a draw with a material, made once
    /// against the device's recorded layouts.
    layouts: Option<(wgpu::BindGroupLayout, wgpu::PipelineLayout)>,
    /// This frame's draws with a material, by command.
    draws: HashMap<u32, u32>,
}

fn upload(device: &wgpu::Device, queue: &wgpu::Queue, image: &HifiPackImage) -> wgpu::Texture {
    let levels = u32::try_from(image.levels.len().max(1)).unwrap_or(1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("art pack material"),
        size: wgpu::Extent3d {
            width: image.width.max(1),
            height: image.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: pack_image::format(image, pack_image::samples_blocks(device)),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    pack_image::write(queue, &texture, 0, image, image.levels.len());
    texture
}

impl Materials {
    /// The pipeline layout of a draw with a material, once made.
    #[must_use]
    pub fn pipeline_layout(&self) -> Option<&wgpu::PipelineLayout> {
        self.layouts.as_ref().map(|(_, l)| l)
    }

    /// How many draws this frame draws with a material.
    #[must_use]
    pub fn drawn_count(&self) -> u64 {
        self.draws.len() as u64
    }

    /// The bytes the materials' pictures take on the device.
    #[must_use]
    pub fn resident_bytes(&self) -> u64 {
        self.gpu.as_ref().map_or(0, |g| g.bytes)
    }

    /// Forget this frame's draws.
    pub fn clear(&mut self) {
        self.draws.clear();
    }

    /// Put `materials` on the device, unless they are there already, and make the layouts.
    pub fn upload(&mut self, cx: &SidecarContext<'_>, materials: &Arc<HifiMaterials>) {
        let device = cx.device();
        let (maps_layout, _) = self.layouts.get_or_insert_with(|| {
            let texture = |binding: u32| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            };
            let maps = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("art pack material"),
                entries: &[texture(0), texture(1)],
            });
            debug_assert_eq!(
                MATERIAL_GROUP, 4,
                "the material's group follows the recorded four"
            );
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("art pack material draw"),
                bind_group_layouts: &[
                    Some(cx.uniform_bind_layout()),
                    Some(cx.texture_bind_layout()),
                    Some(cx.texture_bind_layout()),
                    Some(cx.sampler_bind_layout()),
                    Some(&maps),
                ],
                immediate_size: 0,
            });
            (maps, layout)
        });
        if self
            .gpu
            .as_ref()
            .is_some_and(|g| g.serial == materials.serial)
        {
            return;
        }
        let queue = cx.queue();
        let blocks = pack_image::samples_blocks(device);
        let mut textures = Vec::new();
        let mut gpu = Vec::with_capacity(materials.materials.len());
        let mut bytes = 0;
        for m in &materials.materials {
            let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
            let albedo = upload(device, queue, &m.albedo);
            let normal = upload(device, queue, &m.normal);
            let surface = upload(device, queue, &m.surface);
            bytes += [&m.albedo, &m.normal, &m.surface]
                .iter()
                .map(|i| pack_image::device_bytes(i, blocks))
                .sum::<u64>();
            let albedo_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("art pack material color"),
                layout: cx.texture_bind_layout(),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view(&albedo)),
                }],
            });
            let maps = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("art pack material pictures"),
                layout: maps_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view(&normal)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view(&surface)),
                    },
                ],
            });
            gpu.push(GpuMaterial {
                albedo: albedo_group,
                maps,
            });
            textures.extend([albedo, normal, surface]);
        }
        self.gpu = Some(MaterialsGpu {
            serial: materials.serial,
            materials: gpu,
            _textures: textures,
            bytes,
        });
    }

    /// Work out this frame's draws with a material over the commands `range` re-shades, and ask
    /// `pipelines` for the pipelines they are drawn with.
    pub fn plan(
        &mut self,
        tables: &SideTables,
        slots: &[(u32, u32)],
        range: Range<u32>,
        pipelines: &mut PipelineCache,
    ) {
        self.draws.clear();
        let count = self
            .gpu
            .as_ref()
            .map_or(0, |g| u32::try_from(g.materials.len()).unwrap_or(u32::MAX));
        let mut last = None;
        for (cmd, m) in material_draws(tables, slots, range) {
            if m >= count {
                continue;
            }
            self.draws.insert(cmd, m);
            if let Some(note) = tables.draw_note(cmd) {
                let key = material_key(note);
                if last != Some(key) {
                    pipelines.ask(key);
                    last = Some(key);
                }
            }
        }
    }

    /// The action for the draw at `cmd`: drawn with its material where it has one and the
    /// material's pipeline is built; `None` otherwise.
    #[must_use]
    pub fn action<'c>(
        &'c self,
        cmd: u32,
        note: &DrawNote,
        pipelines: &'c PipelineCache,
    ) -> Option<DrawAction<'c>> {
        let m = *self.draws.get(&cmd)?;
        let material = self.gpu.as_ref()?.materials.get(m as usize)?;
        let pipeline = pipelines.get(&material_key(note))?;
        Some(DrawAction::Retextured {
            pipeline,
            texture: &material.albedo,
            extra: &material.maps,
        })
    }
}
