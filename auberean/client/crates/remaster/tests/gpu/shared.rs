//! Behaviour: none (experimental)
//!
//! One Vulkan device that wgpu and the remaster's layer share: the retail device draws on it every
//! station byte for byte as on a device of its own, and the remaster's picture, lent to wgpu,
//! reads back through wgpu byte for byte as the remaster draws it on a device of its own.

use auberean_remaster::shared::{open, Painter};

use crate::common::{frame, records, turn, STATIONS};

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

/// **On a device it shares with the remaster the retail device draws as on its own, and the
/// remaster's picture reaches wgpu unchanged**: at every station, the retail frame on the shared
/// device is the one the retail device draws on a device of its own, byte for byte; and the picture
/// the remaster lends wgpu reads back as the remaster's own on a device of its own, byte for byte.
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
    // The painter draws the remaster's look, as a player who ticks it sees it.
    alone.look();
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
        let lent = painter
            .paint(&f, (W, H), &mut load)
            .expect("the remaster's frame");
        let c = read(&device, &queue, &lent);
        let d = alone
            .render(&f, W, H, &mut load)
            .expect("the remaster alone");
        assert!(
            c == d.rgba,
            "{}: the lent picture differs from the remaster's own",
            s.name
        );
        eprintln!("{}: the same", s.name);
    }
    drop(on_shared);
    drop(painter);
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
