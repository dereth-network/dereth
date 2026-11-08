//! The layer adopts the ordinary renderer's Vulkan device and draws nothing, and the ordinary
//! frame is the same frame: the same pixels, the same submitted draws (the frame digest) and the
//! same counters. The device made with the remaster's request asks for exactly the ordinary
//! request with the flag off, and only more with it on.
//!
//! Behaviour: none (experimental)

use dereth_gfx::vk;
use dereth_gfx::{Device, DeviceDesc, FrameClock, Lent, Native, State, Touch, Use};
use dereth_primitives::{TextureData, TextureFormat};
use dereth_render::device::{
    Backend, DeviceConfig, Gpu, PerDrawConstants, PerFrameConstants, TextureSlot,
};
use dereth_render::pso::{PipelineKey, SurfaceContext};
use dereth_render::surface::{surface_type, Surface, SurfaceHandler};
use dereth_render::vulkan::world_hook::{
    FrameImage, Untouched, WorldFrame, WorldHook, WorldHookError,
};
use dereth_render::{DrawConstants, VertexFormat, Viewport};

const W: u32 = 128;
const H: u32 = 96;
/// The world's rectangle: smaller than the target, as the game view is inside the interface.
const WORLD: Viewport = Viewport {
    x: 16,
    y: 8,
    width: 96,
    height: 64,
};

/// A Vulkan device, checked by the validation layer where it is installed: an error fails the
/// test that made it.
fn device(remaster: bool) -> Gpu {
    device_checked(remaster, true)
}

fn device_checked(remaster: bool, debug: bool) -> Gpu {
    let gpu = Gpu::new_on(
        Backend::Vulkan,
        None,
        &DeviceConfig {
            width: W,
            height: H,
            remaster,
            debug,
            ..DeviceConfig::default()
        },
    )
    .expect("a Vulkan device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    gpu
}

/// A four-by-four texture of one colour.
fn plain(b: u8, g: u8, r: u8, a: u8) -> TextureData {
    TextureData {
        width: 4,
        height: 4,
        format: TextureFormat::Bgra8,
        levels: vec![[b, g, r, a].repeat(16)],
    }
}

/// The ordinary renderer's pipeline for a textured, depth-tested surface.
fn key(depth_write: bool) -> PipelineKey {
    let mut k = PipelineKey::state_from_surface(
        &Surface {
            r#type: surface_type::BASE1_IMAGE,
            handler: SurfaceHandler::Database,
            ..Surface::default()
        },
        SurfaceContext {
            vertex_format: VertexFormat::XyzDiffuseTex1,
            texture_is_set: true,
            texture_has_palette: false,
            two_sided: true,
            fog_enabled: false,
            lighting: false,
            ..SurfaceContext::default()
        },
    )
    .key;
    k.z_write = depth_write;
    k
}

/// Two triangles from `(x0, y0)` to `(x1, y1)` in clip space at depth `z`, white.
fn quad(x0: f32, y0: f32, x1: f32, y1: f32, z: f32) -> Vec<u8> {
    let mut v = Vec::new();
    for (x, y, u, t) in [
        (x0, y0, 0.0f32, 1.0f32),
        (x1, y0, 1.0, 1.0),
        (x1, y1, 1.0, 0.0),
        (x0, y0, 0.0, 1.0),
        (x1, y1, 1.0, 0.0),
        (x0, y1, 0.0, 0.0),
    ] {
        for f in [x, y, z] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        v.extend_from_slice(&u.to_le_bytes());
        v.extend_from_slice(&t.to_le_bytes());
    }
    v
}

/// One frame as the client draws one: the world in its rectangle (a backdrop, and a nearer
/// square over part of it), the world's end, then the interface over the whole target (a square
/// across the world's edge, depth-tested against the world). Its pixels, digest and counters.
fn frame(gpu: &mut Gpu, [back, near, ui]: [TextureSlot; 3]) -> (Vec<u8>, Option<u64>, String) {
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let per_frame = PerFrameConstants {
        view_proj: identity,
        view: identity,
        fog_params: [0.0, 1.0, 0.0, 0.0],
        ..PerFrameConstants::default()
    };
    let draw = PerDrawConstants::identity();
    let calls = gpu.draw_calls();
    gpu.set_frame_digest(true);
    gpu.begin_frame().expect("begin");
    gpu.set_viewport(WORLD);
    gpu.bind_texture(back, 1);
    let c = DrawConstants::default();
    gpu.draw_dynamic(
        &key(true),
        &c,
        &per_frame,
        &draw,
        &quad(-1.0, -1.0, 1.0, 1.0, 0.9),
    )
    .expect("draw");
    gpu.bind_texture(near, 1);
    gpu.draw_dynamic(
        &key(true),
        &c,
        &per_frame,
        &draw,
        &quad(-0.5, -0.5, 0.25, 0.75, 0.4),
    )
    .expect("draw");
    gpu.world_end(WORLD);
    gpu.reset_viewport();
    // The interface's square is farther than the world's near square and nearer than its
    // backdrop: where the world kept its depth, the near square shows through.
    gpu.bind_texture(ui, 1);
    gpu.draw_dynamic(
        &key(false),
        &c,
        &per_frame,
        &draw,
        &quad(-0.3, -0.9, 0.9, 0.3, 0.6),
    )
    .expect("draw");
    gpu.end_frame().expect("end");
    let pixels = gpu.capture().expect("capture").to_rgba();
    let digest = gpu.last_frame_digest();
    let counters = format!(
        "draw_calls={} live_textures={} descriptors={:?} frames={}",
        gpu.draw_calls() - calls,
        gpu.live_textures(),
        gpu.descriptor_stats(),
        gpu.frame_stamp()
    );
    (pixels, digest, counters)
}

/// The frame's three textures, uploaded once, as a scene keeps its textures from frame to frame.
fn textures(gpu: &mut Gpu) -> [TextureSlot; 3] {
    [
        plain(255, 0, 0, 255),
        plain(0, 200, 0, 255),
        plain(0, 0, 220, 255),
    ]
    .map(|t| gpu.upload_texture(&t).expect("upload"))
}

/// A hook that adopts the device with the layer and draws nothing: it lends the layer the
/// frame's images and has its graph hand them back in the layouts the interface resumes in.
struct Adopted {
    gfx: Device,
    frames: u64,
}

impl Adopted {
    fn new(gpu: &Gpu) -> Self {
        let n = gpu
            .remaster_device()
            .expect("the device has the remaster's features");
        let native = Native {
            entry: n.entry,
            instance: n.instance,
            physical_device: n.physical_device,
            device: n.device,
            queue: n.queue,
            queue_family: n.queue_family,
            ray_queries: n.ray_queries,
        };
        // SAFETY: the handles are the live device's, made with every feature the layer needs,
        // and the hook holding the layer is dropped before the device.
        let gfx = unsafe { Device::adopt(native, &DeviceDesc::default()) }.expect("adopts");
        Self { gfx, frames: 0 }
    }
}

fn lent(img: &FrameImage) -> Lent {
    Lent {
        image: img.image,
        view: img.view,
        format: dereth_gfx::Format::from_vk(img.format).expect("a format the layer knows"),
        width: img.width,
        height: img.height,
        state: State {
            layout: img.layout,
            stages: img.stages,
            access: img.access,
        },
    }
}

impl WorldHook for Adopted {
    fn record(&mut self, f: &WorldFrame<'_>) -> Result<(), WorldHookError> {
        self.frames += 1;
        let err = |e: dereth_gfx::Error| WorldHookError(e.to_string());
        // SAFETY: the images are the frame's, live for the frame, in the states it says.
        let (color, depth) = unsafe {
            (
                self.gfx.lend_texture(&lent(&f.color)).map_err(err)?,
                self.gfx.lend_texture(&lent(&f.depth)).map_err(err)?,
            )
        };
        // SAFETY: the frame's buffer is recording outside any pass on the adopted device, and
        // its owner has finished every frame up to `stamp - in_flight`.
        let mut r = unsafe {
            self.gfx.record_into(
                f.cmd,
                FrameClock {
                    stamp: f.stamp,
                    in_flight: f.in_flight,
                },
            )
        }
        .map_err(err)?;
        r.touch(&[
            Touch::Texture(
                color,
                Use::External(State {
                    layout: f.color.leave_in,
                    stages: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    access: vk::AccessFlags2::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                }),
            ),
            Touch::Texture(
                depth,
                Use::External(State {
                    layout: f.depth.leave_in,
                    stages: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    access: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                        | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                }),
            ),
        ])
        .map_err(err)?;
        r.finish().map_err(err)?;
        self.gfx.destroy_texture(color);
        self.gfx.destroy_texture(depth);
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// The same frame on a device made without the remaster's request, on one made with it, through
/// a hook that only hands the images back, through the layer adopted behind a hook that draws
/// nothing, and once the hook is taken: every pixel, the submitted draws and the counters are
/// the ordinary frame's, and the frame is split only while a hook is installed.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn the_ordinary_frame_is_unchanged_with_the_layer_adopted_and_drawing_nothing() {
    let (off_px, off_digest, off_counts) = {
        let mut gpu = device(false);
        assert!(!gpu.remaster_requested());
        assert!(gpu.remaster_device().is_none());
        assert_eq!(
            gpu.install_world_hook(Box::new(Untouched::default())),
            Err(dereth_render::vulkan::world_hook::WorldHookRefused::NotRequested)
        );
        let t = textures(&mut gpu);
        frame(&mut gpu, t)
    };
    assert!(off_digest.is_some(), "the Vulkan device digests its frames");
    // The world's near square shows through the interface's: the world's depth survived.
    let px = |rgba: &[u8], x: u32, y: u32| {
        let i = ((y * W + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    };
    assert_eq!(
        px(&off_px, 60, 45),
        [0, 200, 0],
        "the near square over the interface"
    );
    assert_eq!(
        px(&off_px, 100, 50),
        [220, 0, 0],
        "the interface over the backdrop"
    );

    let mut gpu = device(true);
    assert!(
        gpu.remaster_supported(),
        "{} has Vulkan 1.3",
        gpu.adapter_name()
    );
    let t = textures(&mut gpu);
    let mut runs = Vec::new();
    runs.push(("with the request", frame(&mut gpu, t)));
    gpu.install_world_hook(Box::new(Untouched::default()))
        .expect("installs");
    runs.push((
        "through a hook that hands the images back",
        frame(&mut gpu, t),
    ));
    let untouched = gpu
        .take_world_hook()
        .expect("installed")
        .as_any_mut()
        .downcast_mut::<Untouched>()
        .map(|u| u.handed);
    assert_eq!(untouched, Some(1));
    gpu.install_world_hook(Box::new(Adopted::new(&gpu)))
        .expect("installs");
    runs.push((
        "through the layer adopted, drawing nothing",
        frame(&mut gpu, t),
    ));
    runs.push(("through it again", frame(&mut gpu, t)));
    let adopted = gpu
        .world_hook_mut()
        .expect("installed")
        .as_any_mut()
        .downcast_mut::<Adopted>()
        .map(|a| a.frames);
    assert_eq!(adopted, Some(2), "the hook drew each frame's world");
    assert!(gpu.world_hook_failed().is_none());
    drop(gpu.take_world_hook());
    runs.push(("after the hook is taken", frame(&mut gpu, t)));
    for (what, (px, digest, counts)) in &runs {
        let changed = px.iter().zip(&off_px).filter(|(a, b)| a != b).count();
        assert_eq!(changed, 0, "{what}: bytes differ");
        assert_eq!(*digest, off_digest, "{what}: the submitted frame differs");
        // The frame stamp counts on from the frames before, so only the rest is compared.
        let strip = |c: &str| c.rsplit_once(" frames=").map(|(a, _)| a.to_owned());
        assert_eq!(strip(counts), strip(&off_counts), "{what}: the counters");
    }
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    crate::no_validation_errors();
}

/// With the flag off the device asks for exactly the ordinary request: a Vulkan 1.2 instance,
/// and of the 1.2 features timeline semaphores alone, with nothing of 1.1 or 1.3. With it on,
/// on a device that has them, the instance is 1.3 and the device asks for the same extensions
/// with the remaster's features added.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn the_device_request_is_the_ordinary_one_with_the_flag_off() {
    let ordinary = {
        let gpu = Gpu::new_on(
            Backend::Vulkan,
            None,
            &DeviceConfig {
                width: W,
                height: H,
                ..DeviceConfig::default()
            },
        )
        .expect("a Vulkan device");
        let Gpu::Vulkan(v) = &gpu else {
            panic!("a Vulkan device");
        };
        v.device_request().lines().to_vec()
    };
    let off = {
        let gpu = device_checked(false, false);
        let Gpu::Vulkan(v) = &gpu else {
            panic!("a Vulkan device");
        };
        v.device_request().lines().to_vec()
    };
    assert_eq!(off, ordinary);
    assert_eq!(off.len(), 4, "{off:#?}");
    assert!(off[0].starts_with("instance 1.2 "), "{}", off[0]);
    assert_eq!(
        off[3],
        format!(
            "{:?}",
            vk::PhysicalDeviceVulkan12Features::default().timeline_semaphore(true)
        )
    );
    let (on, traces) = {
        let gpu = device_checked(true, false);
        let traces = gpu.remaster_device().is_some_and(|d| d.ray_queries);
        let Gpu::Vulkan(v) = &gpu else {
            panic!("a Vulkan device");
        };
        (v.device_request().lines().to_vec(), traces)
    };
    assert_eq!(on.len(), 6, "{on:#?}");
    assert_eq!(
        on[0].replacen("instance 1.3 ", "instance 1.2 ", 1),
        off[0],
        "the instance differs only by its version"
    );
    // The same device extensions, and ray queries' where the device has them.
    let rays = [
        "VK_KHR_acceleration_structure",
        "VK_KHR_ray_query",
        "VK_KHR_deferred_host_operations",
    ];
    let expected = if traces {
        format!("device extensions {rays:?}")
    } else {
        off[1].clone()
    };
    assert_eq!(on[1], expected, "the device extensions");
    assert_ne!(on[2], off[2], "the core features add the indirect draws");
    assert_ne!(on[3], off[3], "the 1.2 features add descriptor indexing");
}
