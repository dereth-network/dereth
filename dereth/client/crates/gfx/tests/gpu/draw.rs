//! What the layer draws lands on the pixels it should: a clear and two triangles, an
//! indirect-count draw that draws exactly the commands its count says, and a copy of one texture
//! into another.
//!
//! Behaviour: none (experimental)

use dereth_gfx::{
    BufferDesc, BufferUsage, ColorAttachment, ColorTarget, Cull, Format, GraphicsDesc, IndexType,
    Load, Location, RasterPass, TextureDesc, TextureUsage, Touch, Use,
};

use crate::shaders::Entry;

/// The push constants of the solid triangle: its color, then its three corners in clip space.
fn triangle(color: [f32; 4], corners: [[f32; 2]; 3]) -> Vec<u8> {
    color
        .iter()
        .chain(corners.iter().flatten())
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

/// The two triangles that make the rectangle from `(x0, y0)` to `(x1, y1)`, in clip space.
fn rectangle(color: [f32; 4], [x0, y0, x1, y1]: [f32; 4]) -> [Vec<u8>; 2] {
    [
        triangle(color, [[x0, y0], [x1, y0], [x0, y1]]),
        triangle(color, [[x0, y1], [x1, y0], [x1, y1]]),
    ]
}

fn pipeline(
    d: &mut dereth_gfx::Device,
    label: &str,
    vertex: Entry,
    fragment: Entry,
) -> dereth_gfx::Pipeline {
    let (vs, fs) = (crate::spirv(vertex), crate::spirv(fragment));
    d.create_graphics_pipeline(&GraphicsDesc {
        label,
        vertex: crate::shader(vertex, &vs),
        fragment: Some(crate::shader(fragment, &fs)),
        color: &[ColorTarget {
            format: Format::Rgba8Unorm,
            blend: None,
            write_alpha: true,
        }],
        depth: None,
        cull: Cull::None,
    })
    .expect("the pipeline builds")
}

fn rect_pipeline(d: &mut dereth_gfx::Device) -> dereth_gfx::Pipeline {
    pipeline(d, "rect", Entry::TriangleVsMain, Entry::TriangleFsMain)
}

fn target(d: &mut dereth_gfx::Device, w: u32, h: u32) -> dereth_gfx::Texture {
    d.create_texture(&TextureDesc {
        label: "target",
        width: w,
        height: h,
        mips: 1,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::COLOR_TARGET | TextureUsage::TRANSFER_SRC,
    })
    .expect("a target")
}

/// A target cleared to black with a red rectangle drawn over it reads back with exactly the
/// rectangle's pixels red, where the clip-space corners put them (y up, as the ordinary renderer
/// has it), and every other pixel black.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn a_rectangle_reads_back_with_exactly_its_pixels_drawn() {
    let mut d = crate::hardware();
    let p = rect_pipeline(&mut d);
    let t = target(&mut d, 64, 64);
    {
        let mut r = d.begin().expect("a recording");
        r.raster(
            &RasterPass {
                name: "rect",
                color: &[ColorAttachment {
                    texture: t,
                    load: Load::Clear([0.0, 0.0, 0.0, 1.0]),
                }],
                depth: None,
                touches: &[],
                area: None,
            },
            |draw| {
                draw.set_pipeline(p)?;
                // x from -0.5 to 0.5 is columns 16 to 48; y from -0.25 to 0.75 is rows 8 to 40,
                // counted from the top.
                for half in rectangle([1.0, 0.0, 0.0, 1.0], [-0.5, -0.25, 0.5, 0.75]) {
                    draw.push(0, &half)?;
                    draw.draw(3, 1, 0, 0);
                }
                Ok(())
            },
        )
        .expect("the pass records");
        let ticket = r.submit().expect("submits");
        d.wait(ticket).expect("finishes");
    }
    let px = crate::pixels(&d.read_texture(t, 0).expect("reads back"));
    let red = px.iter().filter(|p| **p == [255, 0, 0, 255]).count();
    let black = px.iter().filter(|p| **p == [0, 0, 0, 255]).count();
    assert_eq!(red, 32 * 32);
    assert_eq!(black, 64 * 64 - 32 * 32);
    let at = |x: usize, y: usize| px[y * 64 + x];
    assert_eq!(
        at(16, 8),
        [255, 0, 0, 255],
        "the rectangle's top-left pixel"
    );
    assert_eq!(at(47, 39), [255, 0, 0, 255], "its bottom-right pixel");
    assert_eq!(at(15, 8), [0, 0, 0, 255]);
    assert_eq!(at(16, 40), [0, 0, 0, 255]);
    crate::no_validation_errors();
}

/// Eight indexed draw commands, one column each by the draw item each names as its first
/// instance, with a count of five written by a fill before the pass: exactly the first five
/// columns are drawn, whatever the other three say.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn an_indirect_count_draw_draws_exactly_the_counted_commands() {
    let mut d = crate::hardware();
    let p = pipeline(&mut d, "items", Entry::ItemsVsMain, Entry::ItemsFsMain);
    let t = target(&mut d, 64, 8);
    // Item k: column k, x from -1 + k/4 to -0.75 + k/4 (eight pixels), the full height, green.
    let mut rows = Vec::new();
    for k in 0u8..8 {
        let x = -1.0 + f32::from(k) * 0.25;
        for f in [x, -1.0, x + 0.25, 1.0, 0.0, 1.0, 0.0, 1.0] {
            rows.extend_from_slice(&f32::to_le_bytes(f));
        }
    }
    let items = d
        .create_buffer(&BufferDesc {
            label: "items",
            size: rows.len() as u64,
            usage: BufferUsage::STORAGE,
            location: Location::Gpu,
        })
        .expect("a buffer");
    d.write_buffer(items, 0, &rows).expect("staged");
    let items_index = d.bind_buffer(items).expect("an index");
    let indices = d
        .create_buffer(&BufferDesc {
            label: "indices",
            size: 6 * 4,
            usage: BufferUsage::INDEX,
            location: Location::Gpu,
        })
        .expect("a buffer");
    let words: Vec<u8> = (0u32..6).flat_map(u32::to_le_bytes).collect();
    d.write_buffer(indices, 0, &words).expect("staged");
    // An indexed draw command in the device's layout: six indices, once, from the first; command
    // k draws item k.
    let mut commands = Vec::new();
    for k in 0u32..8 {
        for w in d.indexed_command(6, 1, 0, 0, k) {
            commands.extend_from_slice(&w.to_le_bytes());
        }
    }
    let stride = d.indexed_command_words() * 4;
    let args = d
        .create_buffer(&BufferDesc {
            label: "commands",
            size: commands.len() as u64,
            usage: BufferUsage::INDIRECT,
            location: Location::Gpu,
        })
        .expect("a buffer");
    d.write_buffer(args, 0, &commands).expect("staged");
    let count = d
        .create_buffer(&BufferDesc {
            label: "count",
            size: 4,
            usage: BufferUsage::INDIRECT | BufferUsage::TRANSFER_DST,
            location: Location::Gpu,
        })
        .expect("a buffer");
    {
        let mut r = d.begin().expect("a recording");
        r.fill_buffer(count, 0, 4, 5).expect("the count");
        r.raster(
            &RasterPass {
                name: "columns",
                color: &[ColorAttachment {
                    texture: t,
                    load: Load::Clear([0.0, 0.0, 0.0, 1.0]),
                }],
                depth: None,
                touches: &[
                    Touch::Buffer(indices, Use::Index),
                    Touch::Buffer(args, Use::Indirect),
                    Touch::Buffer(count, Use::Indirect),
                    Touch::Buffer(items, Use::StorageRead),
                ],
                area: None,
            },
            |draw| {
                draw.set_pipeline(p)?;
                draw.push(0, &items_index.to_le_bytes())?;
                draw.bind_index_buffer(indices, 0, IndexType::U32)?;
                draw.draw_indexed_indirect_count(args, 0, count, 0, 8, stride)?;
                Ok(())
            },
        )
        .expect("the pass records");
        let ticket = r.submit().expect("submits");
        d.wait(ticket).expect("finishes");
    }
    let px = crate::pixels(&d.read_texture(t, 0).expect("reads back"));
    let green = px.iter().filter(|p| **p == [0, 255, 0, 255]).count();
    assert_eq!(green, 5 * 8 * 8, "five columns of 8 x 8");
    for y in 0..8 {
        assert_eq!(px[y * 64 + 39], [0, 255, 0, 255], "the fifth column");
        assert_eq!(
            px[y * 64 + 40],
            [0, 0, 0, 255],
            "the sixth column is not drawn"
        );
    }
    crate::no_validation_errors();
}

/// A texture copied into another of its size and format reads back with every pixel the first
/// has: a red rectangle on black, copied, is the same rectangle on the same black.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn a_copied_texture_reads_back_with_every_pixel_of_its_source() {
    let mut d = crate::hardware();
    let p = rect_pipeline(&mut d);
    let t = target(&mut d, 64, 64);
    let u = d
        .create_texture(&TextureDesc {
            label: "copy",
            width: 64,
            height: 64,
            mips: 1,
            format: Format::Rgba8Unorm,
            usage: TextureUsage::TRANSFER_DST | TextureUsage::TRANSFER_SRC,
        })
        .expect("a copy");
    {
        let mut r = d.begin().expect("a recording");
        r.raster(
            &RasterPass {
                name: "rect",
                color: &[ColorAttachment {
                    texture: t,
                    load: Load::Clear([0.0, 0.0, 0.0, 1.0]),
                }],
                depth: None,
                touches: &[],
                area: None,
            },
            |draw| {
                draw.set_pipeline(p)?;
                for half in rectangle([1.0, 0.0, 0.0, 1.0], [-0.5, -0.25, 0.5, 0.75]) {
                    draw.push(0, &half)?;
                    draw.draw(3, 1, 0, 0);
                }
                Ok(())
            },
        )
        .expect("the pass records");
        r.copy_texture(t, u).expect("the copy records");
        let ticket = r.submit().expect("submits");
        d.wait(ticket).expect("finishes");
    }
    let source = d.read_texture(t, 0).expect("reads back");
    let copy = d.read_texture(u, 0).expect("reads back");
    assert_eq!(
        crate::pixels(&copy)
            .iter()
            .filter(|p| **p == [255, 0, 0, 255])
            .count(),
        32 * 32
    );
    assert_eq!(copy, source);
    crate::no_validation_errors();
}

/// A pass with an area clears only its area: a target cleared red by one pass, then cleared
/// green by a pass whose area is a square inside it, reads back with exactly the square green and
/// every other pixel still red.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn a_pass_with_an_area_clears_only_its_area() {
    let mut d = crate::hardware();
    let t = target(&mut d, 64, 64);
    {
        let mut r = d.begin().expect("a recording");
        for (load, area) in [
            (Load::Clear([1.0, 0.0, 0.0, 1.0]), None),
            (
                Load::Clear([0.0, 1.0, 0.0, 1.0]),
                Some(dereth_gfx::Rect {
                    x: 16,
                    y: 8,
                    width: 32,
                    height: 24,
                }),
            ),
        ] {
            r.raster(
                &RasterPass {
                    name: "clear",
                    color: &[ColorAttachment { texture: t, load }],
                    depth: None,
                    touches: &[],
                    area,
                },
                |_| Ok(()),
            )
            .expect("the pass records");
        }
        let ticket = r.submit().expect("submits");
        d.wait(ticket).expect("finishes");
    }
    let px = crate::pixels(&d.read_texture(t, 0).expect("reads back"));
    let green = px.iter().filter(|p| **p == [0, 255, 0, 255]).count();
    let red = px.iter().filter(|p| **p == [255, 0, 0, 255]).count();
    assert_eq!(green, 32 * 24, "the area");
    assert_eq!(red, 64 * 64 - 32 * 24, "the rest");
    let at = |x: usize, y: usize| px[y * 64 + x];
    assert_eq!(at(16, 8), [0, 255, 0, 255], "the area's top-left pixel");
    assert_eq!(at(47, 31), [0, 255, 0, 255], "its bottom-right pixel");
    assert_eq!(at(15, 8), [255, 0, 0, 255]);
    assert_eq!(at(16, 32), [255, 0, 0, 255]);
    crate::no_validation_errors();
}
