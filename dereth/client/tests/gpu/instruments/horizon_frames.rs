//! Frames of the Horizon interface over the world at Holtburg, drawn through the client's own
//! presentation on a hardware device: the HUD over the sample player, with a few windows open, at
//! 1920 by 1080 and 2560 by 1440 and at the interface scales 100%, 125% and 150% (a scale the
//! window does not offer is drawn at the one it falls back to, and the file says which).
//!
//! Each frame is written to `DERETH_TEST_HORIZON_DUMP` (default: `horizon/` in cargo's scratch
//! folder for integration tests) as `<width>x<height>-<percent>-<windows>.png`, beside
//! `<...>.tsv`: every quad of its draw list, the picture it samples (an atlas of the interface's
//! own art and its scale, a glyph page, a game icon or a flat fill), how much larger than its
//! texels it is drawn, and how far off the whole pixel its corner stands; and the log says how
//! many of the quads the overlay drew texel for texel. They assert nothing; the claims are the
//! interface's and its drawing's own tests.

#![cfg(gpu)]

use std::io::Write;
use std::sync::Arc;

use dereth_client_runtime::present::Presentation;
use dereth_client_runtime::scene::SceneConfig;
use dereth_client_shell::gpu::Renderer;
use dereth_horizon::art::AcArt as _;
use dereth_horizon::draw::{DrawList, Overlay};
use dereth_horizon::options::{HorizonOptions, StartScreen};
use dereth_render::device::AdapterKind;

/// The window sizes and the interface scales drawn.
const SIZES: [(u32, u32); 2] = [(1920, 1080), (2560, 1440)];
const SCALES: [f32; 3] = [1.0, 1.25, 1.5];

/// The window sets drawn: a name for the file and the windows open.
const SETS: [(&str, &[&str]); 3] = [
    ("hud", &[]),
    ("character-inventory", &["character", "inventory"]),
    ("settings-spellbook", &["options", "actions"]),
];

/// The frames run before the one kept: the windows open and finish fading in, and the pictures
/// drawn at another size than their own are made.
const FRAMES: usize = 30;

fn out_dir() -> String {
    std::env::var("DERETH_TEST_HORIZON_DUMP")
        .unwrap_or_else(|_| format!("{}/horizon", env!("CARGO_TARGET_TMPDIR")))
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    let f = std::fs::File::create(path).expect("create the png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().expect("png header");
    wr.write_image_data(rgba).expect("png data");
    eprintln!("wrote {path}");
}

/// The interface's own pieces, to name what a quad of an atlas samples: each atlas's picture id
/// and scale, and every piece's rectangle in each scale's atlas.
struct Names {
    atlases: Vec<(u32, u32)>,
    manifest: dereth_horizon::pieces::Manifest,
}

impl Names {
    fn new(art: &dereth_horizon::art::Art) -> Self {
        let pieces = dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in");
        let manifest = dereth_horizon::pieces::Manifest::parse(
            pieces
                .get(dereth_horizon::pieces::MANIFEST)
                .expect("the manifest"),
        )
        .expect("the manifest reads");
        // A piece in every scale's atlas, each asked for at exactly that scale's pixels.
        let atlases = manifest.scales.iter().filter_map(|s| {
            let at = manifest.pieces["slot.empty"].at.get(&s.to_string())?;
            #[allow(clippy::cast_precision_loss)]
            let px = at[3] as f32 / manifest.pieces["slot.empty"].size[0];
            Some((art.piece("slot.empty", px)?.tex, *s))
        });
        Self {
            atlases: atlases.collect(),
            manifest,
        }
    }

    /// The piece `q` samples, and its atlas's scale.
    fn of(&self, q: &dereth_horizon::draw::Quad) -> Option<String> {
        let scale = self.atlases.iter().find(|(t, _)| Some(*t) == q.tex)?.1;
        let key = scale.to_string();
        let (cx, cy) = (q.src.x + q.src.w / 2.0, q.src.y + q.src.h / 2.0);
        self.manifest.pieces.iter().find_map(|(name, piece)| {
            let at = piece.at.get(&key)?;
            #[allow(clippy::cast_precision_loss)]
            let (x, y, w, h) = (at[1] as f32, at[2] as f32, at[3] as f32, at[4] as f32);
            (cx >= x && cy >= y && cx <= x + w && cy <= y + h).then(|| format!("{name}@{scale}x"))
        })
    }
}

/// What a quad samples, for the table.
fn picture(
    art: &dereth_horizon::art::Art,
    names: &Names,
    q: &dereth_horizon::draw::Quad,
) -> String {
    let Some(tex) = q.tex else {
        return "fill".to_owned();
    };
    if let Some(piece) = names.of(q) {
        return format!("piece:{piece}");
    }
    match art.image(tex) {
        Some(im) if im.width == 2048 && im.height == 2048 && is_glyph_page(&im) => {
            format!("glyphs:{tex}")
        }
        Some(im) => format!("icon:{}x{}", im.width, im.height),
        None => "missing".to_owned(),
    }
}

/// A glyph page is white throughout, its coverage in the alpha.
fn is_glyph_page(im: &dereth_horizon::art::Image) -> bool {
    im.bgra
        .as_chunks::<4>()
        .0
        .iter()
        .step_by(4099)
        .all(|p| p[0] == 255 && p[1] == 255 && p[2] == 255)
}

/// The draw list's quads, one row each.
fn table(art: &dereth_horizon::art::Art, list: &DrawList, path: &str) {
    let names = Names::new(art);
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).expect("the table"));
    writeln!(
        f,
        "picture\tdst_x\tdst_y\tdst_w\tdst_h\tsrc_w\tsrc_h\tgrow_x\tgrow_y\toff_x\toff_y"
    )
    .expect("write");
    let off = |v: f32| (v - v.round()).abs();
    for q in &list.quads {
        let (gx, gy) = if q.tex.is_some() && q.src.w > 0.0 && q.src.h > 0.0 {
            (q.dst.w / q.src.w, q.dst.h / q.src.h)
        } else {
            (1.0, 1.0)
        };
        writeln!(
            f,
            "{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{gx:.4}\t{gy:.4}\t{:.3}\t{:.3}",
            picture(art, &names, q),
            q.dst.x,
            q.dst.y,
            q.dst.w,
            q.dst.h,
            q.src.w,
            q.src.h,
            off(q.dst.x),
            off(q.dst.y),
        )
        .expect("write");
    }
}

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_TEST_HORIZON_DUMP set and --ignored"]
fn render_the_horizon_frames() {
    let _gpu = crate::common::gpu_lock();
    let dir = out_dir();
    std::fs::create_dir_all(&dir).expect("the dump folder");
    let store = crate::common::dats();
    let (bw, bh) = SIZES[0];
    let mut r = Renderer::new(None, bw, bh).expect("a device");
    assert_eq!(
        r.gpu.adapter_kind(),
        AdapterKind::Hardware,
        "{} is a software rasteriser",
        r.gpu.adapter_name()
    );
    eprintln!("drawing on {:?}, {}", r.backend(), r.gpu.adapter_name());
    let mut world = None;
    r.load_world(
        &store,
        SceneConfig {
            landblock: 0xA9B4,
            character: false,
            time_of_day: Some(0.45),
            ..SceneConfig::default()
        },
        &mut world,
    )
    .expect("the world loads");
    for _ in 0..4 {
        r.stream_world(&store, world.as_mut()).expect("stream");
    }
    for (w, h) in SIZES {
        Presentation::resize(&mut r, w, h).expect("resize");
        #[allow(clippy::cast_precision_loss)]
        let screen = (w as f32, h as f32);
        for scale in SCALES {
            for (name, open) in SETS {
                let art = Arc::new(dereth_horizon::art::Art::new(Arc::new(
                    dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in"),
                )));
                art.set_ac_store(Arc::clone(&store));
                let mut ui = dereth_horizon::ui::HorizonUi::new(
                    Arc::clone(&art),
                    HorizonOptions {
                        scale: Some(scale),
                        screen: StartScreen::Game,
                        open: open.iter().map(|s| (*s).to_owned()).collect(),
                        ..HorizonOptions::default()
                    },
                );
                let mut list = DrawList::default();
                let mut input = dereth_horizon::ui::input::InputFrame {
                    mouse: (-100.0, -100.0),
                    ..Default::default()
                };
                let state = dereth_horizon::ui::game::GameState::default();
                let mut overlay = Overlay::default();
                for _ in 0..FRAMES {
                    ui.frame(&mut list, screen, 1.0 / 60.0, &state, &mut input);
                    input.next_frame();
                    input.mouse = (-100.0, -100.0);
                    overlay.compose(&mut r, &art, &list, false, &[]);
                }
                let drawn = ui.layout_scale(screen);
                // How the overlay samples what it draws: texel for texel, or stretched.
                let (mut exact, mut stretched) = (0, 0);
                for item in overlay.items() {
                    if let dereth_client_contract::overlay::OverlayItem::Triangles {
                        sampler,
                        vertices,
                        ..
                    } = item
                    {
                        if sampler.point {
                            exact += vertices.len() / 6;
                        } else {
                            stretched += vertices.len() / 6;
                        }
                    }
                }
                Presentation::start_frame(&mut r).expect("the frame starts");
                Presentation::draw_scene(&mut r, world.as_ref()).expect("the world is drawn");
                overlay.draw(&mut r).expect("the interface is drawn");
                Presentation::end_frame(&mut r).expect("the frame ends");
                let (cw, ch, bgra) = r.capture_bgra().expect("the frame is read back");
                assert_eq!((cw, ch), (w, h));
                overlay.release(&mut r);
                let rgba: Vec<u8> = bgra
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| [p[2], p[1], p[0], 255])
                    .collect();
                let stem = format!(
                    "{dir}/{w}x{h}-{}-{name}",
                    dereth_horizon::scale::label(scale).trim_end_matches('%')
                );
                write_png(&format!("{stem}.png"), w, h, &rgba);
                table(&art, &list, &format!("{stem}.tsv"));
                let made = overlay.stats.pictures_made;
                eprintln!(
                    "{stem}: asked {}, drawn at {}; {exact} quads texel for texel, {stretched} \
                     stretched; {made} pictures made at their drawn size",
                    dereth_horizon::scale::label(scale),
                    dereth_horizon::scale::label(drawn),
                );
            }
        }
    }
}
