//! Acceleration structures: a bottom-level structure of two geometries, placed twice in a
//! top-level one, traced by a compute pass's ray queries with masks, and read back.
//!
//! Behaviour: none (experimental)

use dereth_gfx::{
    BlasGeometry, BufferDesc, BufferUsage, ComputeDesc, IndexType, Location, TlasInstance, Touch,
    Use,
};

use crate::shaders::Entry;

/// The grid of rays cast down.
const SIDE: u32 = 16;

/// Two squares side by side, each two triangles: the first from -1 to 0 across, the second from
/// 0 to 1, both from -1 to 1 up, at height 0. Positions are four floats a vertex.
fn squares() -> (Vec<u8>, Vec<u8>) {
    let mut v = Vec::new();
    for (x0, x1) in [(-1.0f32, 0.0f32), (0.0, 1.0)] {
        for (x, y) in [(x0, -1.0f32), (x1, -1.0), (x1, 1.0), (x0, 1.0)] {
            for f in [x, y, 0.0, 1.0] {
                v.extend(f.to_le_bytes());
            }
        }
    }
    let i: Vec<u8> = [0u32, 1, 2, 0, 2, 3]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    (v, i)
}

/// The words the grid's rays write, traced with mask `mask`.
fn trace(mask: u32) -> Vec<u32> {
    let mut d = crate::hardware();
    if !d.caps().ray_queries {
        eprintln!("{}: no ray queries", d.caps().name);
        return Vec::new();
    }
    let (vertices, indices) = squares();
    let input = BufferUsage::ACCELERATION_INPUT | BufferUsage::STORAGE;
    let vb = d
        .create_buffer(&BufferDesc {
            label: "squares",
            size: vertices.len() as u64,
            usage: input,
            location: Location::Gpu,
        })
        .expect("a buffer");
    let ib = d
        .create_buffer(&BufferDesc {
            label: "square indices",
            size: indices.len() as u64,
            usage: input,
            location: Location::Gpu,
        })
        .expect("a buffer");
    d.write_buffer(vb, 0, &vertices).expect("written");
    d.write_buffer(ib, 0, &indices).expect("written");
    let square = |k: u64, opaque: bool| BlasGeometry {
        vertices: vb,
        vertex_offset: k * 64,
        vertex_stride: 16,
        vertex_count: 4,
        indices: ib,
        index_offset: 0,
        index_type: IndexType::U32,
        triangles: 2,
        opaque,
    };
    let blas = d
        .create_blas("squares", &[square(0, true), square(1, false)])
        .expect("a bottom-level structure");
    let tlas = d.create_tlas("scene", 1).expect("a top-level structure");
    let at = |z: f32| [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, z];
    let instances = [
        TlasInstance {
            blas,
            transform: at(0.0),
            custom: 5,
            mask: 1,
        },
        TlasInstance {
            blas,
            transform: at(5.0),
            custom: 7,
            mask: 2,
        },
    ];
    let words = d
        .create_buffer(&BufferDesc {
            label: "hits",
            size: u64::from(SIDE * SIDE) * 4,
            usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC,
            location: Location::Gpu,
        })
        .expect("a buffer");
    let out = d.bind_buffer(words).expect("an index");
    let cs = crate::spirv(Entry::TraceCsMain);
    let p = d
        .create_compute_pipeline(&ComputeDesc {
            label: "trace",
            shader: crate::shader(Entry::TraceCsMain, &cs),
        })
        .expect("the pipeline builds");
    let back = d
        .create_buffer(&BufferDesc {
            label: "back",
            size: u64::from(SIDE * SIDE) * 4,
            usage: BufferUsage::TRANSFER_DST,
            location: Location::Readback,
        })
        .expect("a buffer");
    {
        let mut r = d.begin().expect("a recording");
        // Two instances into a structure made for one: it grows.
        r.build_tlas(tlas, &instances).expect("built");
        let accel = r.bind_tlas(tlas).expect("an index");
        r.compute(&[Touch::Buffer(words, Use::StorageWrite)], |c| {
            c.set_pipeline(p)?;
            let push: Vec<u8> = [accel, out, SIDE, SIDE, mask]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect();
            c.push(0, &push)?;
            c.dispatch(SIDE / 8, SIDE / 8, 1);
            Ok(())
        })
        .expect("the trace");
        r.copy_buffer(words, 0, back, 0, u64::from(SIDE * SIDE) * 4)
            .expect("copied");
        r.touch(&[Touch::Buffer(back, Use::HostRead)])
            .expect("touched");
        let t = r.submit().expect("submitted");
        d.wait(t).expect("done");
    }
    assert!(
        d.blas_built(blas).expect("held"),
        "built at the recording's head"
    );
    let bytes = d
        .read_buffer(back, 0, u64::from(SIDE * SIDE) * 4)
        .expect("read");
    let stats = d.stats();
    assert_eq!(stats.acceleration_structures, 2);
    assert!(stats.acceleration_bytes > 0);
    d.destroy_tlas(tlas);
    d.destroy_blas(blas);
    drop(d);
    crate::no_validation_errors();
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect()
}

/// **Rays find the instances their mask lets them see, with each hit's instance, geometry and
/// distance**: of a 16 by 16 grid of rays cast down over a square twice the size of two squares
/// side by side, those over the squares hit them (a quarter of the grid), and the rest miss.
/// With every bit of the mask, each hits the instance five up first (index 7, ten minus five
/// away); with only the first, the one at the ground (index 5, ten away); with neither, none.
/// The left square is geometry 0, the right one geometry 1, whose hits a query accepts itself.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn rays_find_the_instances_their_mask_lets_them_see() {
    let all = trace(0xFF);
    if all.is_empty() {
        return;
    }
    let ground = trace(1);
    let none = trace(4);
    let hits = |w: &[u32]| w.iter().filter(|v| **v != 0).count();
    eprintln!(
        "hits: every mask {}, the ground's {}, neither {}",
        hits(&all),
        hits(&ground),
        hits(&none)
    );
    let quarter = (SIDE * SIDE / 4) as usize;
    assert_eq!(hits(&all), quarter);
    assert_eq!(hits(&ground), quarter);
    assert_eq!(hits(&none), 0);
    for (k, (a, g)) in all.iter().zip(&ground).enumerate() {
        if *a == 0 {
            continue;
        }
        let x = (k as u32 % SIDE) as f32 + 0.5;
        let geometry = u32::from(x > SIDE as f32 / 2.0);
        assert_eq!(a & 0xFFF, 8, "the upper instance's index");
        assert_eq!(a >> 16, 500, "five away");
        assert_eq!((a >> 12) & 0xF, geometry);
        assert_eq!(g & 0xFFF, 6, "the ground's");
        assert_eq!(g >> 16, 1000, "ten away");
        assert_eq!((g >> 12) & 0xF, geometry);
    }
}
