//! Behaviour: none (experimental)
//!
//! One Vulkan device that wgpu and the remaster's layer share: the retail device draws on it every
//! station byte for byte as on a device of its own, and the remaster's picture, lent to wgpu,
//! reads back through wgpu byte for byte as the remaster draws it on a device of its own.

use auberean_remaster::shared::{open, Painter};

use crate::common::{frame, records, settled, turn, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// `t` (`W` x `H`, four bytes a texel) read back through wgpu.
fn read(device: &wgpu::Device, queue: &wgpu::Queue, t: &wgpu::Texture) -> Vec<u8> {
    let row = W * 4;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * H),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        t.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(H),
            },
        },
        wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("the picture maps"));
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("the device finishes");
    slice
        .get_mapped_range()
        .expect("the mapped picture")
        .to_vec()
}

/// Where two pictures differ: how many pixels, by at most how much, and within which box.
fn apart(a: &[u8], b: &[u8]) -> (usize, u8, [u32; 4]) {
    let (mut count, mut most, mut lo, mut hi) = (0usize, 0u8, [W, H], [0u32, 0u32]);
    for (k, (x, y)) in a.chunks(4).zip(b.chunks(4)).enumerate() {
        let off = (0..4).map(|i| x[i].abs_diff(y[i])).max().unwrap_or(0);
        if off > 0 {
            count += 1;
            most = most.max(off);
            let k = u32::try_from(k).unwrap_or(u32::MAX);
            let (px, py) = (k % W, k / W);
            lo = [lo[0].min(px), lo[1].min(py)];
            hi = [hi[0].max(px), hi[1].max(py)];
        }
    }
    (count, most, [lo[0], lo[1], hi[0], hi[1]])
}

/// How far the remaster on its own device may draw a frame from the remaster on the shared one:
/// the same, byte for byte, where the devices draw alike; on the Mac, MoltenVK's two devices of
/// the one GPU draw a few pixels a few levels apart, the same frame on one device varying as much
/// from drawing to drawing, with synchronization validation clean and with or without MoltenVK's
/// fast math. Measured over every Mac run so far: at most 49 pixels apart at a station, by at
/// most 13 levels. So there a frame may differ in at most 64 pixels, by at most 16 levels.
const CROSS_DEVICE: (usize, u8) = if cfg!(target_os = "macos") {
    (64, 16)
} else {
    (0, 0)
};

/// **On a device it shares with the remaster the retail device draws as on its own, and the
/// remaster's picture reaches wgpu unchanged**: at every station, the retail frame on the shared
/// device is the one the retail device draws on a device of its own, byte for byte; the picture
/// the remaster lends wgpu reads back through wgpu as the remaster reads the same drawing on that
/// device, byte for byte; and the remaster draws on the shared device as on a device of its own (byte
/// for byte, or within [`CROSS_DEVICE`]). Each at the remaster's identity, each frame drawn until
/// it settles ([`settled`]).
#[test]
fn on_a_shared_device_retail_draws_as_on_its_own_and_the_remasters_picture_reaches_wgpu() {
    let _turn = turn();
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let instance = auberean_remaster::shared::instance().expect("a Vulkan instance");
    let probe = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("a Vulkan adapter");
    let features = auberean_gpu::Renderer::features_on(&probe, true);
    let limits = auberean_gpu::Renderer::limits_on(&probe);
    drop(probe);
    let shared = open(&instance, None, features, &limits).expect("the shared device");
    let (mut painter, adapter, device, queue) = Painter::new(shared).expect("the painter");
    let mut on_shared =
        auberean_gpu::Renderer::on_device(&adapter, device.clone(), queue.clone(), true);
    let mut own = auberean_gpu::Renderer::new().expect("a device of its own");
    let mut alone = auberean_remaster::Remaster::new().expect("the remaster's own device");
    // The painter starts with the remaster's look, as a player who ticks it sees it; here both draw
    // at its identity.
    let r = painter.remaster_mut();
    r.set_ao(dereth_gfx_passes::ao::AoSettings::default());
    r.set_shadows(auberean_remaster::render::ShadowSettings::default());
    r.set_post(dereth_gfx_passes::post::PostSettings::default());
    r.set_sky(auberean_remaster::render::SkySettings::default());
    for s in &STATIONS {
        let f = frame(s);
        let a = own.render(&f, W, H, &mut load).expect("the retail frame");
        let b = on_shared
            .render(&f, W, H, &mut load)
            .expect("the retail frame on the shared device");
        assert!(
            a.rgba == b.rgba,
            "{}: the retail frame differs on the shared device",
            s.name
        );
        // The lent picture, settled, and the same drawing as the remaster reads it on that device.
        let (lent, _) = settled(|| {
            let t = painter
                .paint(&f, (W, H), &mut load)
                .expect("the remaster's frame");
            read(&device, &queue, &t)
        });
        let mine = painter
            .remaster_mut()
            .read_drawn()
            .expect("the remaster's picture on the shared device")
            .expect("a drawing")
            .rgba;
        let (alone_frame, _) = settled(|| {
            alone
                .render(&f, W, H, &mut load)
                .expect("the remaster alone")
                .rgba
        });
        let lending = apart(&lent, &mine);
        let devices = apart(&mine, &alone_frame);
        eprintln!(
            "{}: lent {} pixels from read; devices {} pixels apart, by at most {}, within {:?}",
            s.name, lending.0, devices.0, devices.1, devices.2
        );
        if devices.0 > 0 {
            if let Some(dir) =
                std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from)
            {
                crate::common::write_png(&dir.join(format!("{}-shared.png", s.name)), W, H, &mine);
                crate::common::write_png(
                    &dir.join(format!("{}-own.png", s.name)),
                    W,
                    H,
                    &alone_frame,
                );
            }
        }
        assert!(
            lent == mine,
            "{}: the lent picture differs from the remaster's own on the device: {lending:?}",
            s.name
        );
        assert!(
            devices.0 <= CROSS_DEVICE.0 && devices.1 <= CROSS_DEVICE.1,
            "{}: the remaster draws apart on the two devices: {devices:?}",
            s.name
        );
    }
    drop(on_shared);
    drop(painter);
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
