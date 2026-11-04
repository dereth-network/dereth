//! An art pack's materials drawn in place of retail pictures.
//!
//! The scene names, each frame, the texture slots that hold a retail picture one of the pack's
//! materials stands for ([`HifiFrame::material_slots`]). In a re-shaded frame every draw whose
//! first stage's texture is such a slot is drawn with the material: the pipeline derived from
//! its own for a material ([`Variant::Material`]), the material's base color, if it has one,
//! bound in place of the slot and its other pictures in a group of their own. Everything else
//! about the draw -- its geometry and coordinates, its blend, cut-out and depth, its lights -- is
//! the recorded draw's, so the retail surface's flags are kept.
//!
//! The draws of objects an art pack has models for ([`Mark::ArtPackCovered`]) are never drawn
//! with a material: the pack's model stands in for the whole object. The retail look draws no
//! material at all: the materials are drawn only while the art pack is.
//!
//! A material with no base color, which keeps the retail picture, is relief alone, and is put on
//! the device and drawn only while the Depth Mapping menu asks for any. Its level is a constant
//! the materials share, so it changes with no pipeline or picture made again.
//!
//! A frame that is not re-shaded draws the retail pictures, as does a draw whose material
//! pipeline is still being built.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use crate::derive::cache::PipelineCache;
use crate::derive::material::{level_constants, MATERIAL_GROUP, ROOM_LIGHTS};
use crate::derive::{DerivedKey, Variant};
use crate::shared::pack_image;
use crate::snapshot::{HifiLight, HifiMaterials, HifiPackImage};
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
        && frame.materials.as_ref().is_some_and(|m| {
            m.materials
                .iter()
                .any(|m| settings.depth_mapping > 0 || m.albedo.is_some() || m.ice)
        })
}

/// One material on the device: its base color in the recorded texture group's layout, and its
/// other pictures in the material's group.
#[derive(Debug)]
struct GpuMaterial {
    albedo: Option<wgpu::BindGroup>,
    /// Whether it is ice, which is drawn with or without depth mapping.
    ice: bool,
    maps: wgpu::BindGroup,
}

/// The materials of one pack on the device.
#[derive(Debug)]
struct MaterialsGpu {
    serial: u64,
    materials: Vec<GpuMaterial>,
    _textures: Vec<wgpu::Texture>,
    /// How each material's grit is laid over it.
    _tiles: Option<wgpu::Buffer>,
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
    /// The depth mapping's constants on the device, and the level they hold.
    level: Option<(wgpu::Buffer, u8)>,
    /// The rooms' lights on the device, and what they hold.
    rooms: Option<(wgpu::Buffer, Vec<u8>)>,
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

/// The rooms' lights as the material variant reads them: how many, then where each is and how far
/// it reaches, then how bright.
#[must_use]
pub fn rooms_constants(lights: &[(HifiLight, f32)], damp: bool) -> Vec<u8> {
    let mut floats = vec![0.0f32; 4 + ROOM_LIGHTS * 8];
    let lights = &lights[..lights.len().min(ROOM_LIGHTS)];
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: at most ROOM_LIGHTS.
    {
        floats[0] = lights.len() as f32;
    }
    floats[1] = f32::from(u8::from(damp));
    for (k, (light, reach)) in lights.iter().enumerate() {
        let place = 4 + k * 4;
        floats[place] = light.position.x;
        floats[place + 1] = light.position.y;
        floats[place + 2] = light.position.z;
        floats[place + 3] = *reach;
        let [r, g, b] = light.color;
        floats[4 + ROOM_LIGHTS * 4 + k * 4] =
            (0.2126 * r + 0.7152 * g + 0.0722 * b) * light.intensity;
    }
    floats.iter().flat_map(|v| v.to_le_bytes()).collect()
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

    /// Put the rooms' `lights`, each with how far it reaches, and whether surfaces show their
    /// `damp`, on the device where they changed.
    pub fn rooms(&mut self, cx: &SidecarContext<'_>, lights: &[(HifiLight, f32)], damp: bool) {
        let bytes = rooms_constants(lights, damp);
        let (buffer, held) = self.room_buffer(cx);
        if *held != bytes {
            cx.queue().write_buffer(buffer, 0, &bytes);
            *held = bytes;
        }
    }

    fn room_buffer(&mut self, cx: &SidecarContext<'_>) -> &mut (wgpu::Buffer, Vec<u8>) {
        self.rooms.get_or_insert_with(|| {
            let buffer = cx.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("art pack material rooms"),
                size: 16 + (ROOM_LIGHTS as u64) * 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            (buffer, Vec::new())
        })
    }

    /// Put `materials` on the device, unless they are there already, and make the layouts.
    pub fn upload(&mut self, cx: &SidecarContext<'_>, materials: &Arc<HifiMaterials>, level: u8) {
        let device = cx.device();
        let (level_buffer, held) = self.level.get_or_insert_with(|| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("art pack material level"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            (buffer, u8::MAX)
        });
        if *held != level {
            let bytes: Vec<u8> = level_constants(level)
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            cx.queue().write_buffer(level_buffer, 0, &bytes);
            *held = level;
        }
        let level_buffer = level_buffer.clone();
        let rooms_buffer = self.room_buffer(cx).0.clone();
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
                entries: &[
                    texture(0),
                    texture(1),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(16),
                        },
                        count: None,
                    },
                    texture(3),
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(32),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(16 + (ROOM_LIGHTS as u64) * 32),
                        },
                        count: None,
                    },
                ],
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
        // The grit pictures, and for a material with none a flat picture no strength reaches.
        let mut grits: Vec<wgpu::Texture> = materials
            .grits
            .iter()
            .map(|g| upload(device, queue, g))
            .collect();
        bytes += materials
            .grits
            .iter()
            .map(|g| pack_image::device_bytes(g, blocks))
            .sum::<u64>();
        let flat = upload(
            device,
            queue,
            &HifiPackImage {
                width: 1,
                height: 1,
                levels: vec![Arc::new(vec![128; 4])],
                ..HifiPackImage::default()
            },
        );
        let grit_views: Vec<wgpu::TextureView> = grits
            .iter()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
            .collect();
        let flat_view = flat.create_view(&wgpu::TextureViewDescriptor::default());
        grits.push(flat);
        let grit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("art pack material grit"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..wgpu::SamplerDescriptor::default()
        });
        // How each material's grit is laid over it, one buffer at the offsets uniforms may start at.
        let stride = 32.max(device.limits().min_uniform_buffer_offset_alignment as usize);
        let mut tile_bytes = vec![0u8; stride * materials.materials.len().max(1)];
        for (m, at) in materials
            .materials
            .iter()
            .zip(tile_bytes.chunks_mut(stride))
        {
            let on = if m.grit.is_some() { 1.0f32 } else { 0.0 };
            let floors = if m.damp_floors { 1.0f32 } else { 0.0 };
            let slime = if m.slime { 1.0f32 } else { 0.0 };
            let ice = if m.ice { 1.0f32 } else { 0.0 };
            for (v, to) in [m.grit_repeat[0], m.grit_repeat[1], on, floors, slime, ice]
                .iter()
                .zip(at.chunks_mut(4))
            {
                to.copy_from_slice(&v.to_le_bytes());
            }
        }
        let tiles = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("art pack material tiles"),
            size: tile_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&tiles, 0, &tile_bytes);
        for (index, m) in materials.materials.iter().enumerate() {
            let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
            let albedo = m.albedo.as_ref().map(|a| upload(device, queue, a));
            let normal = upload(device, queue, &m.normal);
            let surface = upload(device, queue, &m.surface);
            bytes += m
                .albedo
                .iter()
                .chain([&m.normal, &m.surface])
                .map(|i| pack_image::device_bytes(i, blocks))
                .sum::<u64>();
            let albedo_group = albedo.as_ref().map(|albedo| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("art pack material color"),
                    layout: cx.texture_bind_layout(),
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view(albedo)),
                    }],
                })
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: level_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(
                            m.grit
                                .and_then(|g| grit_views.get(g as usize))
                                .unwrap_or(&flat_view),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&grit_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &tiles,
                            offset: (stride * index) as u64,
                            size: wgpu::BufferSize::new(32),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: rooms_buffer.as_entire_binding(),
                    },
                ],
            });
            gpu.push(GpuMaterial {
                albedo: albedo_group,
                ice: m.ice,
                maps,
            });
            textures.extend(albedo);
            textures.extend([normal, surface]);
        }
        textures.extend(grits);
        self.gpu = Some(MaterialsGpu {
            serial: materials.serial,
            materials: gpu,
            _textures: textures,
            _tiles: Some(tiles),
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
        depth_mapping: u8,
        pipelines: &mut PipelineCache,
    ) {
        self.draws.clear();
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let mut last = None;
        for (cmd, m) in material_draws(tables, slots, range) {
            let Some(material) = gpu.materials.get(m as usize) else {
                continue;
            };
            let note = tables.draw_note(cmd);
            // A material with no base color is relief alone, drawn only with depth mapping on. What
            // it adds says nothing of a blended draw: a sprite's glow or a pane of glass stays as
            // the retail picture draws it. Ice is its own look.
            if material.albedo.is_none()
                && !material.ice
                && (depth_mapping == 0 || note.is_some_and(|n| n.key.alpha_blend))
            {
                continue;
            }
            self.draws.insert(cmd, m);
            if let Some(note) = note {
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
            texture: material.albedo.as_ref(),
            extra: &material.maps,
        })
    }
}
