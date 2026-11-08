//! The remaster hooked into the ordinary renderer's Vulkan or Direct3D 12 device: it adopts the
//! device's handles with the backend layer, and draws each frame's world where the world ends,
//! into the frame's own images, for the interface to draw over.
//!
//! This is the one module of the crate that calls the layer's `unsafe` entry points: adopting a
//! device it did not make, recording into a command buffer it was lent, and lending the layer the
//! frame's images. Each call says why it holds.

#![allow(unsafe_code)]

use std::any::Any;

use dereth_gfx::{Device, DeviceDesc, FrameClock, Lent, Native, Rect, State, Touch, Use};
use dereth_render::vulkan::world_hook::{
    FrameImage, RemasterDevice, WorldFrame, WorldHook, WorldHookError,
};

use crate::assets::AssetSource;
use crate::render::{color_back, depth_back, Renderer};
use crate::scene::{FrameDelta, Receiver, SceneDelta};

/// The remaster, as the device's world hook.
#[derive(Debug)]
pub struct Remaster {
    renderer: Renderer,
    /// Frames whose world it drew.
    pub frames: u64,
}

fn hook_error(e: dereth_gfx::Error) -> WorldHookError {
    WorldHookError(e.to_string())
}

impl Remaster {
    /// The remaster on `device`, the ordinary renderer's device: the layer adopts it, and every
    /// pipeline is made now.
    ///
    /// # Errors
    /// What the layer or the device refuses.
    pub fn adopt(device: &RemasterDevice) -> Result<Self, dereth_gfx::Error> {
        let native = Native {
            entry: device.entry.clone(),
            instance: device.instance.clone(),
            physical_device: device.physical_device,
            device: device.device.clone(),
            queue: device.queue,
            queue_family: device.queue_family,
            ray_queries: device.ray_queries,
        };
        let depth = dereth_gfx::Format::from_vk(device.depth_format).ok_or_else(|| {
            dereth_gfx::Error::Unsupported(format!(
                "the frame's depth format {:?}",
                device.depth_format
            ))
        })?;
        // SAFETY: the handles are the ordinary renderer's live device, which it made with every
        // feature the layer needs (it hands them out only then). The hook, which holds the layer,
        // is dropped by the device before it destroys itself, and it is only driven from the
        // frame's own thread, inside the device's frame.
        let gfx = unsafe { Device::adopt(native, &DeviceDesc::default()) }?;
        Ok(Self {
            renderer: Renderer::new(gfx, depth)?,
            frames: 0,
        })
    }

    /// The remaster on `device`, the ordinary renderer's Direct3D 12 device: the layer adopts
    /// it, and every pipeline is made now.
    ///
    /// # Errors
    /// What the layer or the device refuses.
    #[cfg(windows)]
    pub fn adopt_d3d12(
        device: &dereth_render::d3d12::world_hook::RemasterDevice,
    ) -> Result<Self, dereth_gfx::Error> {
        let native = dereth_gfx::d3d12::D3d12Native {
            device: device.device.clone(),
            queue: device.queue.clone(),
            ray_queries: device.ray_queries,
        };
        let depth = dereth_gfx::d3d12::format_of(device.depth_format).ok_or_else(|| {
            dereth_gfx::Error::Unsupported(format!(
                "the frame's depth format {:?}",
                device.depth_format
            ))
        })?;
        // SAFETY: the handles are the ordinary renderer's live device, which it hands out only
        // when the adapter can draw the remaster. The hook, which holds the layer, is dropped by
        // the device before it goes, and it is only driven from the frame's own thread, inside
        // the device's frame.
        let gfx = unsafe { Device::adopt_d3d12(native, &DeviceDesc::default()) }?;
        Ok(Self {
            renderer: Renderer::new(gfx, depth)?,
            frames: 0,
        })
    }

    /// The renderer.
    pub fn renderer(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    /// Take one frame of the scene, its assets read from `src`; `detail` names the detail
    /// pictures (by id) and tilings the buildings' shells and the rooms' walls are drawn under.
    pub fn receive(
        &mut self,
        deltas: Vec<SceneDelta>,
        frame: FrameDelta,
        detail: [Option<(u32, f32)>; 2],
        src: &mut dyn AssetSource,
    ) {
        self.renderer.receive(deltas, frame, detail, src);
    }
}

/// The layer's view of one of the frame's images.
fn lent(img: &FrameImage) -> Result<Lent, dereth_gfx::Error> {
    Ok(Lent {
        image: img.image,
        view: img.view,
        format: dereth_gfx::Format::from_vk(img.format).ok_or_else(|| {
            dereth_gfx::Error::Unsupported(format!("the format {:?}", img.format))
        })?,
        width: img.width,
        height: img.height,
        state: State {
            layout: img.layout,
            stages: img.stages,
            access: img.access,
        },
    })
}

impl Remaster {
    /// Submit what `plan`'s frame records before its scaler, and run the scaler after it.
    #[cfg(target_os = "macos")]
    fn record_before_scaler(
        &mut self,
        plan: &crate::render::Plan,
    ) -> Result<crate::render::SceneRecorded, WorldHookError> {
        let mut own = self.renderer.gfx.begin().map_err(hook_error)?;
        let recorded = plan.record_scene(&mut own);
        let done = own.submit().map_err(hook_error)?;
        let scene = recorded.map_err(hook_error)?;
        self.renderer.run_scaler(plan, done).map_err(hook_error)?;
        Ok(scene)
    }

    /// Never asked: only a device over Metal has a scaler outside the layer.
    #[cfg(not(target_os = "macos"))]
    #[allow(clippy::unused_self)]
    fn record_before_scaler(
        &mut self,
        _: &crate::render::Plan,
    ) -> Result<crate::render::SceneRecorded, WorldHookError> {
        unreachable!("a frame splits only over Metal")
    }
}

impl WorldHook for Remaster {
    fn record(&mut self, f: &WorldFrame<'_>) -> Result<(), WorldHookError> {
        let area = Rect {
            x: i32::try_from(f.viewport.x).unwrap_or(0),
            y: i32::try_from(f.viewport.y).unwrap_or(0),
            width: f.viewport.width,
            height: f.viewport.height,
        };
        let plan = if area.width > 0 && area.height > 0 {
            self.renderer.prepare(area, f.gamma).map_err(hook_error)?
        } else {
            None
        };
        // What comes before a scaler outside the layer goes first, in a submission of its own, so
        // its work between that and the frame's command buffer is in order on the one queue.
        let scene = match &plan {
            Some(plan) if plan.splits() => Some(self.record_before_scaler(plan)?),
            _ => None,
        };
        let (color_img, depth_img) = (
            lent(&f.color).map_err(hook_error)?,
            lent(&f.depth).map_err(hook_error)?,
        );
        let gfx = &mut self.renderer.gfx;
        // SAFETY: the images are the frame's own, live while the frame is recorded, in the states
        // the frame says; the layer forgets them below, before the frame is submitted.
        let (color, depth) = unsafe {
            (
                gfx.lend_texture(&color_img).map_err(hook_error)?,
                gfx.lend_texture(&depth_img).map_err(hook_error)?,
            )
        };
        let clock = FrameClock {
            stamp: f.stamp,
            in_flight: f.in_flight,
        };
        // SAFETY: the frame's command buffer is recording outside any pass, on the adopted device,
        // and its owner submits it after this; its own ring has finished every frame up to
        // `stamp - in_flight` before handing it out.
        let mut r = unsafe { gfx.record_into(f.cmd, clock) }.map_err(hook_error)?;
        let back = (color_back(f.color.leave_in), depth_back(f.depth.leave_in));
        let recorded = match &plan {
            Some(plan) => {
                for b in &plan.retired {
                    r.destroy_buffer(*b);
                }
                match scene {
                    Some(scene) => plan.record_finish(&mut r, scene, color, depth, back.0, back.1),
                    None => plan.record(&mut r, color, depth, back.0, back.1),
                }
            }
            None => r.touch(&[
                Touch::Texture(color, Use::External(back.0)),
                Touch::Texture(depth, Use::External(back.1)),
            ]),
        };
        let finished = r.finish();
        gfx.destroy_texture(color);
        gfx.destroy_texture(depth);
        recorded.and(finished).map_err(hook_error)?;
        if plan.is_some() {
            self.frames += 1;
        }
        Ok(())
    }

    #[cfg(windows)]
    fn record_d3d12(
        &mut self,
        f: &dereth_render::d3d12::world_hook::WorldFrame<'_>,
    ) -> Result<(), WorldHookError> {
        let area = Rect {
            x: i32::try_from(f.viewport.x).unwrap_or(0),
            y: i32::try_from(f.viewport.y).unwrap_or(0),
            width: f.viewport.width,
            height: f.viewport.height,
        };
        let plan = if area.width > 0 && area.height > 0 {
            self.renderer.prepare(area, f.gamma).map_err(hook_error)?
        } else {
            None
        };
        let lent = |img: &dereth_render::d3d12::world_hook::FrameImage| {
            Ok::<_, dereth_gfx::Error>(dereth_gfx::d3d12::D3d12Lent {
                resource: img.resource.clone(),
                format: dereth_gfx::d3d12::format_of(img.format).ok_or_else(|| {
                    dereth_gfx::Error::Unsupported(format!("the format {:?}", img.format))
                })?,
                width: img.width,
                height: img.height,
                state: img.state,
            })
        };
        let (color_img, depth_img) = (
            lent(&f.color).map_err(hook_error)?,
            lent(&f.depth).map_err(hook_error)?,
        );
        let gfx = &mut self.renderer.gfx;
        // SAFETY: the targets are the frame's own, live while the frame is recorded, in the
        // states the frame says; the layer forgets them below, before the frame is submitted.
        let (color, depth) = unsafe {
            (
                gfx.lend_d3d12_texture(&color_img).map_err(hook_error)?,
                gfx.lend_d3d12_texture(&depth_img).map_err(hook_error)?,
            )
        };
        let clock = FrameClock {
            stamp: f.stamp,
            in_flight: f.in_flight,
        };
        let back = (
            dereth_gfx::d3d12::state(f.color.leave_in),
            dereth_gfx::d3d12::state(f.depth.leave_in),
        );
        // SAFETY: the frame's list is open, on the adopted device, and its owner submits it
        // after this; its own ring has finished every frame up to `stamp - in_flight` before
        // handing it out.
        let recorded = match unsafe { gfx.record_into_d3d12(f.list, clock) } {
            Ok(mut r) => {
                let recorded = match &plan {
                    Some(plan) => {
                        for b in &plan.retired {
                            r.destroy_buffer(*b);
                        }
                        plan.record(&mut r, color, depth, back.0, back.1)
                    }
                    None => Ok(()),
                };
                // The targets go back in the states the frame resumes in, whatever happened.
                let returned = r.touch(&[
                    Touch::Texture(color, Use::External(back.0)),
                    Touch::Texture(depth, Use::External(back.1)),
                ]);
                let finished = r.finish();
                recorded.and(returned).and(finished)
            }
            Err(e) => Err(e),
        };
        gfx.destroy_texture(color);
        gfx.destroy_texture(depth);
        recorded.map_err(hook_error)?;
        if plan.is_some() {
            self.frames += 1;
        }
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Where the scene's published frames wait for the hook: the changes since the last frame taken,
/// in order, and the newest frame's state.
#[derive(Debug, Default)]
pub struct Inbox {
    deltas: Vec<SceneDelta>,
    frame: Option<FrameDelta>,
}

impl Inbox {
    /// The changes waiting and the newest frame, when a frame was published since the last take.
    pub fn take(&mut self) -> Option<(Vec<SceneDelta>, FrameDelta)> {
        let frame = self.frame.take()?;
        Some((std::mem::take(&mut self.deltas), frame))
    }
}

impl Receiver for Inbox {
    fn receive(&mut self, deltas: Vec<SceneDelta>, frame: FrameDelta) {
        self.deltas.extend(deltas);
        self.frame = Some(frame);
    }
}
