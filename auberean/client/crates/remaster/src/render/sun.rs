//! The sun's shadows: the frame's cascades and the sun's share at each pixel are the shared
//! passes' (`dereth_gfx_passes::sun`); what casts into the cascades' maps is the frame's.
//!
//! The casters are the frame's own (`Frame::shadow`), as the retail shadow map's: every object of
//! the detailed blocks at its full detail, each through its first layer's alpha as the retail
//! shadow map takes it. The land casts none: its own vertices know where its hills keep the sun
//! off it, and the retail light already has them. Each cascade takes the objects whose sphere
//! reaches into it, every frame; a moved body's, whose sphere its pose may leave, in every
//! cascade.

use std::sync::Arc;

use auberean_draw::constants::{bound_stages, draw_uniforms, DrawUniforms, FrameUniforms};
use auberean_draw::material::d3d;
use auberean_draw::mesh::{MeshVertex, PosedVertex};
use auberean_draw::picture::Image;
use auberean_draw::scene::Frame;
use dereth_gfx::{Buffer, DepthAttachment, IndexType, Pipeline, RasterPass, Recorder, Rect};
use dereth_gfx_passes::frame::{push_words, Camera, PerFrame};
use dereth_gfx_passes::sun::{cascades, sun_of, MaskPlan, CASCADES};
use glam::Vec3;

use super::{Key, Program, Remaster, RemasterError, ALPHA_KEEP};

/// How the sun's shadows are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShadowSettings {
    /// Whether they are. Off, every surface takes all of the sun retail gives it.
    pub on: bool,
    /// Each cascade's map is this many texels a side.
    pub size: u32,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self {
            on: false,
            size: 2048,
        }
    }
}

/// One object's draw into a cascade's map.
#[derive(Debug, Clone, Copy)]
struct Caster {
    vertices: u32,
    posed: Option<u32>,
    indices: Buffer,
    index_count: u32,
    record: u32,
    texture: u32,
    sampler: u32,
}

/// One cascade's part of the frame: its constants' index, its map and its objects.
#[derive(Debug, Clone)]
struct CascadePlan {
    frame: u32,
    map: dereth_gfx::Texture,
    casters: Vec<Caster>,
}

/// What the sun's shadows draw this frame.
#[derive(Debug, Clone)]
pub(super) struct SunPlan {
    cascades: Vec<CascadePlan>,
    size: u32,
    mesh: Pipeline,
    /// The sun's share's part of the frame.
    pub(super) mask: MaskPlan,
}

impl Remaster {
    /// The sphere round a mesh's vertices, in its own space: worked out once a list.
    fn sphere(&mut self, list: &Arc<Vec<MeshVertex>>) -> (Vec3, f32) {
        let key = (Arc::as_ptr(list) as usize, list.len());
        *self.spheres.entry(key).or_insert_with(|| {
            let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for v in list.iter() {
                let p = Vec3::from(v.position);
                lo = lo.min(p);
                hi = hi.max(p);
            }
            if list.is_empty() {
                return (Vec3::ZERO, 0.0);
            }
            let centre = (lo + hi) * 0.5;
            let radius = list
                .iter()
                .map(|v| (Vec3::from(v.position) - centre).length())
                .fold(0.0f32, f32::max);
            (centre, radius)
        })
    }

    /// Work out the frame's cascades from `camera` and the outdoors' light `out_u`, gather each
    /// one's casters (their constants into `records`, a moved body's pose into `posed`), and write
    /// the cascades' constants and the sun's share's; `area` is the world's size. `None` when the shadows are off or there is no sun to cast
    /// them.
    ///
    /// # Errors
    /// What the device refuses.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn plan_sun(
        &mut self,
        frame: &Frame,
        camera: &Camera,
        out_u: &FrameUniforms,
        records: &mut Vec<DrawUniforms>,
        posed: &mut Vec<PosedVertex>,
        area: (u32, u32),
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Result<Option<SunPlan>, RemasterError> {
        let Some(shadow) = frame.shadow.as_ref().filter(|_| self.shadows.on) else {
            return Ok(None);
        };
        let Some((sun, strength)) =
            sun_of(Vec3::new(out_u.to_sun[0], out_u.to_sun[1], out_u.to_sun[2]))
        else {
            return Ok(None);
        };
        self.sun.ensure(&mut self.gfx, self.shadows.size)?;
        let size = self.sun.size();
        let cs = cascades(camera, sun, size);
        while self.cascade_lights.len() < CASCADES {
            let l = PerFrame::new(
                &mut self.gfx,
                "remaster cascade light",
                4096,
                dereth_gfx::BufferUsage::default(),
            )?;
            self.cascade_lights.push(l);
        }
        // Each cascade's constants: the outdoors' light seen down the sun.
        for (c, cascade) in cs.iter().enumerate() {
            let mut u = *out_u;
            u.view_proj = cascade.view_proj.to_cols_array_2d();
            let bytes = Self::light_rows(&u, cascade.light_view, (size, size));
            self.cascade_lights[c].write(&mut self.gfx, &bytes)?;
        }
        let mut plans: Vec<CascadePlan> = (0..CASCADES)
            .map(|c| CascadePlan {
                frame: self.cascade_lights[c].index,
                map: self.sun.maps()[c].map,
                casters: Vec::new(),
            })
            .collect();
        for b in &shadow.casters {
            if b.indices.is_empty() {
                continue;
            }
            let model = glam::Mat4::from_cols_array_2d(&b.transform.0);
            let (centre, radius) = self.sphere(&b.vertices);
            let scale = model
                .x_axis
                .truncate()
                .length()
                .max(model.y_axis.truncate().length())
                .max(model.z_axis.truncate().length());
            let (centre, radius) = (model.transform_point3(centre), radius * scale);
            let reached: Vec<usize> = (0..CASCADES)
                .filter(|&c| b.posed.is_some() || cs[c].reaches(centre, radius))
                .collect();
            if reached.is_empty() {
                continue;
            }
            let stage = bound_stages(&b.layer)[0];
            let (texture, sampler) = match stage {
                Some(s) => (
                    self.picture(s.texture, load)?,
                    if s.clamp_u || s.clamp_v {
                        self.clamp
                    } else {
                        self.wrap
                    },
                ),
                None => (self.white, self.wrap),
            };
            let record = u32::try_from(records.len()).unwrap_or(u32::MAX);
            records.push(draw_uniforms(
                &b.layer,
                &b.transform,
                b.tint,
                out_u.sun_color,
                &b.points,
            ));
            let vertices = self.mesh_vertices(&b.vertices)?;
            let pose = match &b.posed {
                Some(list) => {
                    let base = u32::try_from(posed.len()).unwrap_or(0);
                    posed.extend_from_slice(list.shared().as_slice());
                    Some(base)
                }
                None => None,
            };
            let indices = self.index_list(&b.indices)?;
            let caster = Caster {
                vertices,
                posed: pose,
                indices,
                index_count: u32::try_from(b.indices.len()).unwrap_or(0),
                record,
                texture,
                sampler,
            };
            for c in reached {
                plans[c].casters.push(caster);
            }
        }
        let mesh = self.pipeline(Key {
            program: Program::MeshCaster,
            src: d3d::BLEND_ONE,
            dst: d3d::BLEND_ZERO,
            depth_test: true,
            depth_write: true,
            cull: d3d::CULL_NONE,
        })?;
        let (mask, _) = self.sun.prepare(&mut self.gfx, &cs, strength, area)?;
        Ok(Some(SunPlan {
            cascades: plans,
            size,
            mesh,
            mask,
        }))
    }
}

impl SunPlan {
    /// Draw each cascade's map. `draws` and `posed` are the frame's constants' and poses' indices
    /// in the table; `white` and `wrap` the picture and sampler a stage without one takes.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(super) fn record_maps(
        &self,
        r: &mut Recorder<'_>,
        draws: u32,
        posed: u32,
        (white, wrap): (u32, u32),
    ) -> Result<(), dereth_gfx::Error> {
        let full = Rect {
            x: 0,
            y: 0,
            width: self.size,
            height: self.size,
        };
        for cp in &self.cascades {
            r.raster(
                &RasterPass {
                    name: "remaster sun cascade",
                    color: &[],
                    depth: Some(DepthAttachment {
                        texture: cp.map,
                        clear: Some(1.0),
                        write: true,
                    }),
                    touches: &[],
                    area: Some(full),
                },
                |d| {
                    if !cp.casters.is_empty() {
                        d.set_pipeline(self.mesh)?;
                    }
                    for c in &cp.casters {
                        d.push(
                            0,
                            &push_words(&[
                                cp.frame,
                                draws,
                                c.record,
                                c.vertices,
                                if c.posed.is_some() { posed } else { u32::MAX },
                                c.posed.unwrap_or(0),
                                c.texture,
                                white,
                                white,
                                c.sampler,
                                wrap,
                                wrap,
                                u32::MAX,
                                ALPHA_KEEP,
                                u32::MAX,
                                0,
                                u32::MAX,
                            ]),
                        )?;
                        d.bind_index_buffer(c.indices, 0, IndexType::U16)?;
                        d.draw_indexed(c.index_count, 1, 0, 0, 0);
                    }
                    Ok(())
                },
            )?;
        }
        Ok(())
    }
}
