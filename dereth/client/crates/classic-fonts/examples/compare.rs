//! The carried fonts against the Windows font system, request by request: advances, glyph
//! pixels, line height and baseline. Windows only.
//!
//! `cargo run -p dereth-classic-fonts --example compare -- [--system-faces] [--png <dir>]
//! [--emit-advances]`
//!
//! - `--system-faces` draws with this machine's own Times New Roman, Courier New and Arial files
//!   through the carried path, which measures the request reading, hinting and drawing apart from
//!   the faces.
//! - `--png <dir>` writes each request's sample lines drawn both ways and their difference.
//! - `--emit-advances` prints the measured advance differences (`src/advances.rs`).
//! - `--atlases <dir> <prefix>` compares atlases drawn elsewhere instead (a browser's, say): for
//!   each request `n` in order, `<prefix>-<n>.alpha` (the atlas's coverage bytes, rows top
//!   first) and `<prefix>-<n>.cells` (a line `line_height baseline`, then one line per character:
//!   `codepoint cell_x cell_y advance`).

#[cfg(not(windows))]
fn main() {
    eprintln!("the comparison needs the Windows font system");
}

#[cfg(windows)]
fn main() {
    windows::main();
}

#[cfg(windows)]
#[path = "../tests/cpu/measure.rs"]
mod measure;

#[cfg(windows)]
mod windows {
    use super::measure::{compare, Tally};
    use dereth_classic_dat::fonts::{cp1252, measure_cells, FontAtlas, FontSpec, REQUESTS};
    use dereth_classic_fonts::{design_advance, rasterize, rasterize_face, Face};

    /// The Windows face file a request draws with.
    fn system_file(spec: &FontSpec) -> &'static str {
        match spec.face.as_str() {
            "Courier New" => "courbd.ttf",
            "Arial" => "arialbd.ttf",
            "Times New Roman Italic" => "timesi.ttf",
            _ if spec.weight >= 600 => "timesbd.ttf",
            _ => "times.ttf",
        }
    }

    fn spec(height: i32, width: i32, weight: i32, face: &str) -> FontSpec {
        FontSpec {
            height,
            width,
            weight,
            italic: false,
            face: face.into(),
        }
    }

    pub fn main() {
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--emit-advances") {
            emit_advances();
            return;
        }
        let system = args.iter().any(|a| a == "--system-faces");
        let atlases = args
            .iter()
            .position(|a| a == "--atlases")
            .map(|i| (args[i + 1].clone(), args[i + 2].clone()));
        let png = args
            .iter()
            .position(|a| a == "--png")
            .and_then(|i| args.get(i + 1))
            .map(std::path::PathBuf::from);
        println!(
            "{:18} {:>9} {:>9} {:>16} {:>9} {:>8} {:>8} {:>8}",
            "request", "line", "baseline", "advance =/1/>1", "same", "px diff", "on/off", "ink err"
        );
        let mut all = Tally::default();
        for (n, (name, height, width, weight, face)) in REQUESTS.into_iter().enumerate() {
            let spec = spec(height, width, weight, face);
            let gdi = dereth_classic_gdi::fonts::rasterize(&spec).unwrap();
            let ours = if let Some((dir, prefix)) = &atlases {
                read_atlas(&format!("{dir}/{prefix}-{n}"))
            } else if system {
                let data =
                    std::fs::read(format!("C:/Windows/Fonts/{}", system_file(&spec))).unwrap();
                let face = Face {
                    data: &data,
                    ..Face::for_request(&spec)
                };
                rasterize_face(&face, &spec).unwrap()
            } else {
                rasterize(&spec).unwrap()
            };
            let t = compare(&gdi, &ours);
            println!(
                "{:18} {:>4}/{:<4} {:>4}/{:<4} {:>5}/{:>4}/{:>4} {:>4}/{:<4} {:>7.1}% {:>7.1}% {:>7.1}%",
                name,
                gdi.line_height,
                ours.line_height,
                gdi.baseline,
                ours.baseline,
                t.exact,
                t.one,
                t.worse,
                t.identical,
                t.glyphs,
                Tally::percent(t.differing, t.pixels),
                Tally::percent(t.on_off, t.pixels),
                t.ink_error(),
            );
            if let Some(dir) = &png {
                write_pair(dir, name, &gdi, &ours);
            }
            all.add(&t);
        }
        println!(
            "{:18} {:>9} {:>9} {:>5}/{:>4}/{:>4} {:>4}/{:<4} {:>7.1}% {:>7.1}% {:>7.1}%",
            "all",
            "",
            "",
            all.exact,
            all.one,
            all.worse,
            all.identical,
            all.glyphs,
            Tally::percent(all.differing, all.pixels),
            Tally::percent(all.on_off, all.pixels),
            all.ink_error(),
        );
    }

    /// An atlas drawn elsewhere: see `--atlases`.
    fn read_atlas(stem: &str) -> FontAtlas {
        let alpha = std::fs::read(format!("{stem}.alpha")).unwrap();
        let text = std::fs::read_to_string(format!("{stem}.cells")).unwrap();
        let mut lines = text.lines();
        let head: Vec<i32> = lines
            .next()
            .unwrap()
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        let cells: Vec<(u32, i32, i32, i32)> = lines
            .map(|l| {
                let v: Vec<i64> = l.split_whitespace().map(|v| v.parse().unwrap()).collect();
                (
                    u32::try_from(v[0]).unwrap(),
                    i32::try_from(v[1]).unwrap(),
                    i32::try_from(v[2]).unwrap(),
                    i32::try_from(v[3]).unwrap(),
                )
            })
            .collect();
        let mut rgba = vec![0xFF; alpha.len() * 4];
        for (px, a) in rgba.as_chunks_mut::<4>().0.iter_mut().zip(&alpha) {
            px[3] = *a;
        }
        let glyphs = measure_cells(&alpha, 1024, &cells, head[1]).unwrap();
        FontAtlas {
            width: 1024,
            height: u32::try_from(alpha.len() / 1024).unwrap(),
            rgba,
            glyphs,
            line_height: head[0],
            baseline: head[1],
            face: stem.into(),
        }
    }

    /// The advances Windows gives each request, as differences from the carried fonts' design
    /// advances scaled and rounded, in the Western code page's order: `src/advances.rs`.
    fn emit_advances() {
        println!("pub(crate) const MEASURED: [(i32, i32, i32, &str, &str); 17] = [");
        for (_, height, width, weight, face) in REQUESTS {
            let spec = spec(height, width, weight, face);
            let gdi = dereth_classic_gdi::fonts::rasterize(&spec).unwrap();
            let mut out = String::new();
            for byte in 32u8..=255 {
                let Some(c) = cp1252(byte).filter(|_| byte != 127) else {
                    continue;
                };
                let d = gdi.glyphs[&u32::from(c)].advance - design_advance(&spec, c).unwrap();
                out.push(match d {
                    0 => '.',
                    1 => '+',
                    -1 => '-',
                    2..=9 => char::from(b'0' + u8::try_from(d).unwrap()),
                    -9..=-2 => char::from(b'a' - 1 + u8::try_from(-d).unwrap()),
                    _ => panic!("{face} {height}-{width} {c:?}: {d}"),
                });
            }
            println!("    ({height}, {width}, {weight}, {face:?}, concat!(");
            for chunk in out.as_bytes().chunks(64) {
                println!("        \"{}\",", std::str::from_utf8(chunk).unwrap());
            }
            println!("    )),");
        }
        println!("];");
    }

    /// Sample lines drawn with both atlases, one above the other, then their difference: red is
    /// Windows' coverage only, green the carried fonts' only, yellow both.
    fn write_pair(dir: &std::path::Path, name: &str, a: &FontAtlas, b: &FontAtlas) {
        std::fs::create_dir_all(dir).unwrap();
        let lines = [
            "The quick brown fox jumps over the lazy dog. 0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZ",
            "Sphinx of black quartz, judge my vow! @#$%&*()[]{}<>?/\\|~^_-+=;:'\",. \u{e9}\u{fc}\u{df}\u{c5}",
        ];
        let w = 960usize;
        let lh = usize::try_from(a.line_height.max(b.line_height)).unwrap() + 2;
        let h = lh * lines.len() * 3;
        let mut img = vec![0u8; w * h * 3];
        for (i, text) in lines.iter().enumerate() {
            let y0 = i * 3 * lh;
            let (ra, rb) = (draw(a, text, w, lh), draw(b, text, w, lh));
            for y in 0..lh {
                for x in 0..w {
                    let (va, vb) = (ra[y * w + x], rb[y * w + x]);
                    let rows = [
                        (y0 + y, [va, va, va]),
                        (y0 + lh + y, [vb, vb, vb]),
                        (y0 + 2 * lh + y, [va, vb, 0]),
                    ];
                    for (row, rgb) in rows {
                        let at = (row * w + x) * 3;
                        img[at..at + 3].copy_from_slice(&rgb);
                    }
                }
            }
        }
        let file = std::fs::File::create(dir.join(format!("{name}.png"))).unwrap();
        let mut enc = png::Encoder::new(
            std::io::BufWriter::new(file),
            u32::try_from(w).unwrap(),
            u32::try_from(h).unwrap(),
        );
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&img).unwrap();
    }

    /// `text` drawn white on black with `atlas`, the pen from x 2 on the line's baseline.
    fn draw(atlas: &FontAtlas, text: &str, w: usize, lh: usize) -> Vec<u8> {
        let aw = i32::try_from(atlas.width).unwrap();
        let mut out = vec![0u8; w * lh];
        let mut pen = 2i32;
        for c in text.chars() {
            let Some(g) = atlas.glyphs.get(&u32::from(c)) else {
                continue;
            };
            for y in 0..g.height {
                for x in 0..g.width {
                    let (px, py) = (pen + g.bearing_x + x, atlas.baseline + g.bearing_y + y);
                    let (Ok(px), Ok(py)) = (usize::try_from(px), usize::try_from(py)) else {
                        continue;
                    };
                    if px >= w || py >= lh {
                        continue;
                    }
                    let at = usize::try_from(((g.y + y) * aw + g.x + x) * 4 + 3).unwrap();
                    let o = &mut out[py * w + px];
                    *o = (*o).max(atlas.rgba[at]);
                }
            }
            pen += g.advance;
        }
        out
    }
}
