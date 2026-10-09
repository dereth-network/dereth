//! Frame writers for the high-fidelity presentation: each station drawn with the presentation
//! off and with one of its features on, written as PNG pairs with a contact sheet and the frame
//! times. They assert nothing; the claims are in `rendering::hifi_*`.
//!
//! - `DERETH_HIFI_CAPTURE_DIR`: where the files go, one folder per feature (default: `hifi/` in
//!   cargo's scratch folder for integration tests).
//! - `DERETH_HIFI_CAPTURE`: a comma list of features, `all`, or `off-only` (the off frames alone,
//!   written straight into the folder, with each recorded frame's digest: what a build before the
//!   presentation can capture). Default `passthrough`.
//! - `DERETH_HIFI_STATIONS`: a comma list of station names, or `all` (the default).
//! - `DERETH_HIFI_QUALITY`: `low`, `medium`, `high` (the default) or `ultra`.
//! - `DERETH_HIFI_SIZE`: `WxH`, default `1920x1080`.
//! - `DERETH_HIFI_TIMING`: `1` times each station at 3840x2160 as well.
//! - `DERETH_HIFI_BASELINE_DIR`: a capture of another build to compare the off frames with.
//!
//! The device is the one `DERETH_TEST_RENDERER` names, a fresh one for every run of a station;
//! the presentation draws on `wgpu` alone.

#![cfg(gpu)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use dereth_render::device::{DeviceConfig, Gpu};

use super::hifi_stations::{self, capture, moved, read_png, write_png, Shot, Station};

/// Frames a station is stepped before it is captured.
const WARM: usize = 6;
/// Frames drawn before timing, and frames timed.
const TIMING_WARM: usize = 30;
const TIMED: usize = 120;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn root() -> PathBuf {
    env("DERETH_HIFI_CAPTURE_DIR")
        .or_else(|| env("DERETH_TEST_HIFI_DUMP"))
        .map_or_else(
            || Path::new(env!("CARGO_TARGET_TMPDIR")).join("hifi"),
            PathBuf::from,
        )
}

fn size() -> (u32, u32) {
    env("DERETH_HIFI_SIZE")
        .and_then(|s| {
            let (w, h) = s.split_once(['x', 'X'])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or((1920, 1080))
}

/// The features asked for.
fn features() -> Vec<String> {
    let raw = env("DERETH_HIFI_CAPTURE").unwrap_or_else(|| "passthrough".into());
    if raw.trim().eq_ignore_ascii_case("all") {
        return FEATURES.iter().map(|(n, _)| (*n).to_owned()).collect();
    }
    raw.split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Each feature this build can capture, and the `[Fidelity]` option that turns it on (`Debug`
/// views take their own value; every other option takes the quality level).
const FEATURES: &[(&str, &str)] = &[
    ("passthrough", "Debug=1"),
    ("depth", "Debug=2"),
    // The world re-shaded and written back through the neutral resolve.
    ("reshade", "Lighting"),
    ("normals", "Debug=3"),
    ("census", "Debug=6"),
    ("atmosphere", "Sky"),
    // Per-pixel sun and sky light, bloom and the tone map.
    ("lighting", "Lighting"),
    ("shadows", "Lighting,Shadows"),
    ("lighting-shadow", "Lighting=3,Debug=5"),
    ("gtao", "AmbientOcclusion"),
    ("ao", "Debug=4,AmbientOcclusion=3"),
    // Bounced light, over the lighting.
    ("gi", "Lighting,Shadows,GlobalIllumination"),
    ("gi-indirect", "Lighting=3,GlobalIllumination=3,Debug=4"),
    // Outdoor lamps, lanterns, torches and braziers lit after dusk.
    ("lamps", "Lighting,Shadows,Lamps"),
];

/// The diagnostic views written beside each feature's frames, as `<station>-<view>.png`.
const VIEWS: &[(&str, &[&str])] = &[
    ("passthrough", &["depth"]),
    ("reshade", &["normals", "census"]),
    ("lighting", &["lighting-shadow"]),
    ("gtao", &["ao"]),
    ("gi", &["gi-indirect"]),
];

fn quality() -> u32 {
    match env("DERETH_HIFI_QUALITY")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "low" => 1,
        "medium" => 2,
        "ultra" => 4,
        _ => 3,
    }
}

fn wanted(station: &Station) -> bool {
    env("DERETH_HIFI_STATIONS").is_none_or(|list| {
        list.eq_ignore_ascii_case("all")
            || list
                .split(',')
                .any(|s| s.trim().eq_ignore_ascii_case(station.name))
    })
}

fn device(width: u32, height: u32) -> Gpu {
    let cfg = DeviceConfig {
        width,
        height,
        #[cfg(feature = "hifi")]
        hifi: true,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new(None, &cfg)
        .unwrap_or_else(|e| panic!("the capture needs a device ({width}x{height}): {e}"));
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "captures are taken on the hardware adapter, not {}",
        gpu.adapter_name()
    );
    gpu
}

/// The CPU milliseconds of one frame, averaged over [`TIMED`] frames drawn as they stand, after
/// [`TIMING_WARM`] frames; the device is waited for at the end so the frames are all counted.
fn time_frames(shot: &mut Shot, gpu: &mut Gpu) -> f64 {
    for _ in 0..TIMING_WARM {
        shot.draw(gpu);
    }
    let started = Instant::now();
    for _ in 0..TIMED {
        shot.draw(gpu);
    }
    gpu.wait_idle().expect("the device finishes");
    #[allow(clippy::cast_precision_loss)] // a small frame count
    let ms = started.elapsed().as_secs_f64() * 1000.0 / TIMED as f64;
    ms
}

/// `rgba` at a quarter of its size or smaller, box-filtered, `tw` wide.
fn thumbnail(rgba: &[u8], width: u32, height: u32, tw: u32) -> (u32, Vec<u8>) {
    let th = (u64::from(height) * u64::from(tw) / u64::from(width.max(1))).max(1);
    #[allow(clippy::cast_possible_truncation)] // a thumbnail's height
    let th = th as u32;
    let mut out = vec![0u8; (tw * th * 4) as usize];
    for y in 0..th {
        for x in 0..tw {
            let (x0, x1) = (
                x * width / tw,
                ((x + 1) * width / tw).max(x * width / tw + 1),
            );
            let (y0, y1) = (
                y * height / th,
                ((y + 1) * height / th).max(y * height / th + 1),
            );
            let mut sum = [0u32; 4];
            for sy in y0..y1.min(height) {
                for sx in x0..x1.min(width) {
                    let i = ((sy * width + sx) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += u32::from(rgba[i + c]);
                    }
                }
            }
            let n = ((x1 - x0) * (y1 - y0)).max(1);
            let o = ((y * tw + x) * 4) as usize;
            for c in 0..4 {
                #[allow(clippy::cast_possible_truncation)] // an average of bytes
                {
                    out[o + c] = (sum[c] / n) as u8;
                }
            }
        }
    }
    (th, out)
}

/// The off and on frames of every station side by side, one station a row.
fn write_sheet(path: &Path, rows: &[(Vec<u8>, Vec<u8>)], width: u32, height: u32) {
    const TW: u32 = 480;
    if rows.is_empty() {
        return;
    }
    let (th, _) = thumbnail(&rows[0].0, width, height, TW);
    let sheet_w = TW * 2 + 12;
    #[allow(clippy::cast_possible_truncation)] // nine rows
    let sheet_h = (th + 4) * rows.len() as u32 + 4;
    let mut sheet = vec![32u8; (sheet_w * sheet_h * 4) as usize];
    for (r, (off, on)) in rows.iter().enumerate() {
        for (c, img) in [off, on].into_iter().enumerate() {
            let (_, t) = thumbnail(img, width, height, TW);
            #[allow(clippy::cast_possible_truncation)] // nine rows, two columns
            let (ox, oy) = (4 + c as u32 * (TW + 4), 4 + r as u32 * (th + 4));
            for y in 0..th {
                let src = ((y * TW) * 4) as usize;
                let dst = (((oy + y) * sheet_w + ox) * 4) as usize;
                sheet[dst..dst + (TW * 4) as usize]
                    .copy_from_slice(&t[src..src + (TW * 4) as usize]);
            }
        }
    }
    write_png(path, sheet_w, sheet_h, &sheet);
}

/// The presentation's `[Fidelity]` settings for `feature`, in a build that has it.
#[cfg(feature = "hifi")]
fn fidelity_for(feature: &str) -> Option<dereth_client_runtime::render_prefs::FidelityPreferences> {
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    // Every box ticked, as the options page ticks them: outdoors the ambient occlusion stands
    // aside for the bounced light.
    if feature == "all" {
        return FidelityPreferences::parse_switch(EVERY_BOX).ok();
    }
    // A list written out, `+` between the options: `spec-Lighting=1+Shadows=1`. A box is 1, as
    // the page ticks it; a higher number asks for that quality level.
    if let Some(list) = feature.strip_prefix("spec-") {
        return FidelityPreferences::parse_switch(&list.replace('+', ",")).ok();
    }
    let (_, spec) = FEATURES.iter().find(|(n, _)| *n == feature)?;
    let spec = if spec.contains('=') {
        (*spec).to_owned()
    } else {
        spec.split(',')
            .map(|name| format!("{name}={}", quality()))
            .collect::<Vec<_>>()
            .join(",")
    };
    dereth_client_runtime::render_prefs::FidelityPreferences::parse_switch(&spec).ok()
}

/// Every box ticked, as the options page ticks them.
#[cfg(feature = "hifi")]
pub(crate) const EVERY_BOX: &str =
    "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Lamps=1,Sky=1";

/// Wait for the presentation's pipelines, so the frames captured are its whole picture.
fn settle(gpu: &mut Gpu) {
    #[cfg(feature = "hifi")]
    assert!(
        gpu.hifi_settle(std::time::Duration::from_secs(60)),
        "the presentation's pipelines were not built in a minute"
    );
    #[cfg(not(feature = "hifi"))]
    let _ = gpu;
}

/// One station's off or on run: the last frame, its digest and counts, and the frame time.
struct Run {
    rgba: Vec<u8>,
    digest: Option<u64>,
    counts: String,
    cpu_ms: Option<f64>,
    cpu_ms_4k: Option<f64>,
    gpu_ms: Vec<(&'static str, f32)>,
    /// The presentation's passes at the capture size, when timed; `gpu_ms` is then at 4K when
    /// that was timed too.
    gpu_ms_frame: Vec<(&'static str, f32)>,
}

/// One run of `station` on a device of its own, so what an earlier station left on a device
/// cannot reach this one's frame.
fn run(
    store: &std::sync::Arc<dereth_dat::RetailDatStore>,
    (width, height): (u32, u32),
    station: &Station,
    feature: Option<&str>,
    timing: bool,
) -> Run {
    let mut device = device(width, height);
    let gpu = &mut device;
    let mut shot = Shot::open(store, gpu, station);
    #[cfg(feature = "hifi")]
    if let Some(prefs) = feature.and_then(fidelity_for) {
        shot.scene.draw.cfg.render.fidelity = prefs;
    }
    #[cfg(not(feature = "hifi"))]
    let _ = feature;
    shot.poll(gpu);
    gpu.set_frame_digest(true);
    let warm = env("DERETH_HIFI_WARM")
        .and_then(|w| w.parse().ok())
        .unwrap_or(WARM);
    for _ in 1..warm {
        shot.step(gpu);
        settle(gpu);
    }
    let counts = hifi_stations::counted_step(&mut shot, gpu);
    let digest = gpu.last_frame_digest();
    #[cfg(feature = "hifi")]
    if feature.is_some() {
        eprintln!(
            "  {}: failed {:?}; passes {:?}",
            station.name,
            gpu.hifi_failed(),
            gpu.hifi_report().map(|r| r.passes)
        );
    }
    let rgba = capture(gpu);
    #[cfg(feature = "hifi")]
    if let Some(e) = gpu.hifi_failed() {
        eprintln!("{}: the presentation failed: {e}", station.name);
    }
    gpu.set_frame_digest(false);
    let cpu_ms = timing.then(|| time_frames(&mut shot, gpu));
    #[cfg(feature = "hifi")]
    let gpu_ms_frame = gpu.hifi_report().map(|r| r.gpu_ms).unwrap_or_default();
    #[cfg(not(feature = "hifi"))]
    let gpu_ms_frame = Vec::new();
    let cpu_ms_4k = (timing && env("DERETH_HIFI_TIMING").as_deref() == Some("1")).then(|| {
        let (w, h) = gpu.size();
        gpu.resize(3840, 2160).expect("4K");
        let ms = time_frames(&mut shot, gpu);
        gpu.resize(w, h).expect("back");
        ms
    });
    #[cfg(feature = "hifi")]
    let gpu_ms = gpu.hifi_report().map(|r| r.gpu_ms).unwrap_or_default();
    #[cfg(not(feature = "hifi"))]
    let gpu_ms = Vec::new();
    #[cfg(feature = "hifi")]
    {
        shot.scene.draw.cfg.render.fidelity = Default::default();
        shot.poll(gpu);
    }
    Run {
        rgba,
        digest,
        counts,
        cpu_ms,
        cpu_ms_4k,
        gpu_ms,
        gpu_ms_frame,
    }
}

/// The boxes alone and together, every station drawn once off and once per combination, each on
/// a device of its own: `<root>/<combination>/<station>-on.png`, the off frames in
/// `<root>/presets/`, a contact sheet of every station across every combination, and the frame
/// times at the capture size and at 4K (CPU per frame, GPU per pass) in
/// `<root>/presets/timing.csv`.
///
/// `DERETH_HIFI_PRESETS` is a comma list of `spec-<Name=value+...>` and `all` (every box); the
/// default is each box that stands alone, each box drawn inside the lighting over it, and all.
#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_HIFI_CAPTURE_DIR set and --ignored"]
#[allow(clippy::too_many_lines)]
fn capture_the_presets() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (width, height) = size();
    let (backend, adapter) = {
        let gpu = device(width, height);
        (gpu.backend().name(), gpu.adapter_name().to_owned())
    };
    let presets: Vec<String> = env("DERETH_HIFI_PRESETS").map_or_else(
        || {
            [
                "spec-lighting=1",
                "spec-ambientocclusion=1",
                "spec-sky=1",
                "spec-lighting=1+shadows=1",
                "spec-lighting=1+globalillumination=1",
                "spec-lighting=1+lamps=1",
                "all",
            ]
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
        },
        |list| {
            list.split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect()
        },
    );
    let timing = env("DERETH_HIFI_TIMING").as_deref() == Some("1");
    eprintln!("capturing presets on {adapter} ({backend}) at {width}x{height}");
    let stations: Vec<Station> = hifi_stations::stations(&store)
        .into_iter()
        .chain(hifi_stations::close_stations(&store))
        .filter(wanted)
        .collect();
    let base = root().join("presets");
    std::fs::create_dir_all(&base).expect("the capture folder");
    let mut csv = String::from(
        "station,preset,size,cpu_ms_off,cpu_ms_on,cpu_ms_off_4k,cpu_ms_on_4k,gpu_ms_total,gpu_ms_total_4k,pass_ms,pass_ms_4k\n",
    );
    let mut rows: Vec<Vec<Vec<u8>>> = Vec::new();
    for station in &stations {
        let off = run(&store, (width, height), station, None, timing);
        write_png(
            &base.join(format!("{}-off.png", station.name)),
            width,
            height,
            &off.rgba,
        );
        let mut row = vec![off.rgba.clone()];
        for preset in &presets {
            let dir = root().join(preset);
            std::fs::create_dir_all(&dir).expect("the capture folder");
            let on = run(&store, (width, height), station, Some(preset), timing);
            write_png(
                &dir.join(format!("{}-on.png", station.name)),
                width,
                height,
                &on.rgba,
            );
            let changed = moved(&off.rgba, &on.rgba);
            let total = |v: &[(&str, f32)]| v.iter().map(|(_, ms)| ms).sum::<f32>();
            let list = |v: &[(&str, f32)]| {
                v.iter()
                    .map(|(n, ms)| format!("{n}={ms:.4}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let opt = |v: Option<f64>| v.map_or_else(String::new, |v| format!("{v:.4}"));
            eprintln!(
                "{} {preset}: {changed} bytes differ; cpu {} / {} ms; gpu {:.3} ms",
                station.name,
                opt(off.cpu_ms),
                opt(on.cpu_ms),
                total(&on.gpu_ms_frame)
            );
            let four_k = on.cpu_ms_4k.is_some();
            let _ = writeln!(
                csv,
                "{},{preset},{width}x{height},{},{},{},{},{:.4},{:.4},\"{}\",\"{}\"",
                station.name,
                opt(off.cpu_ms),
                opt(on.cpu_ms),
                opt(off.cpu_ms_4k),
                opt(on.cpu_ms_4k),
                total(&on.gpu_ms_frame),
                if four_k { total(&on.gpu_ms) } else { 0.0 },
                list(&on.gpu_ms_frame),
                if four_k {
                    list(&on.gpu_ms)
                } else {
                    String::new()
                },
            );
            row.push(on.rgba);
        }
        rows.push(row);
        // Written as it goes, so a long run can be looked at before it ends.
        std::fs::write(base.join("timing.csv"), &csv).expect("timing.csv");
    }
    std::fs::write(
        base.join("columns.txt"),
        format!("off,{}\n{adapter}\n{backend}\n", presets.join(",")),
    )
    .expect("columns.txt");
    write_grid(&base.join("sheet.png"), &rows, width, height);
    eprintln!("wrote {}", base.display());
}

/// Each row's frames side by side, one station a row.
fn write_grid(path: &Path, rows: &[Vec<Vec<u8>>], width: u32, height: u32) {
    const TW: u32 = 320;
    let Some(first) = rows.first() else {
        return;
    };
    let (th, _) = thumbnail(&first[0], width, height, TW);
    #[allow(clippy::cast_possible_truncation)] // a few columns and rows
    let (cols, n) = (first.len() as u32, rows.len() as u32);
    let sheet_w = (TW + 4) * cols + 4;
    let sheet_h = (th + 4) * n + 4;
    let mut sheet = vec![32u8; (sheet_w * sheet_h * 4) as usize];
    for (r, row) in rows.iter().enumerate() {
        for (c, img) in row.iter().enumerate() {
            let (_, t) = thumbnail(img, width, height, TW);
            #[allow(clippy::cast_possible_truncation)] // a few columns and rows
            let (ox, oy) = (4 + c as u32 * (TW + 4), 4 + r as u32 * (th + 4));
            for y in 0..th {
                let src = ((y * TW) * 4) as usize;
                let dst = (((oy + y) * sheet_w + ox) * 4) as usize;
                sheet[dst..dst + (TW * 4) as usize]
                    .copy_from_slice(&t[src..src + (TW * 4) as usize]);
            }
        }
    }
    write_png(path, sheet_w, sheet_h, &sheet);
}

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_HIFI_CAPTURE_DIR set and --ignored"]
#[allow(clippy::too_many_lines)]
fn capture_the_stations() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (width, height) = size();
    let (backend, adapter) = {
        let gpu = device(width, height);
        (gpu.backend().name(), gpu.adapter_name().to_owned())
    };
    eprintln!("capturing on {adapter} ({backend}) at {width}x{height}");
    let stations: Vec<Station> = hifi_stations::stations(&store)
        .into_iter()
        .chain(hifi_stations::close_stations(&store))
        .filter(wanted)
        .collect();
    let baseline = env("DERETH_HIFI_BASELINE_DIR").map(PathBuf::from);
    for feature in features() {
        let off_only = feature == "off-only";
        let dir = if off_only {
            root()
        } else {
            root().join(&feature)
        };
        std::fs::create_dir_all(&dir).expect("the capture folder");
        let mut sheet = Vec::new();
        let mut digests = String::new();
        let mut counts = String::new();
        let mut csv = String::from(
            "station,backend,adapter,driver,size,cpu_ms_off,cpu_ms_on,cpu_ms_off_4k,cpu_ms_on_4k,gpu_ms_total,pass_ms\n",
        );
        let mut stations_txt = String::new();
        for station in &stations {
            let _ = writeln!(stations_txt, "{}: {}", station.name, station.why);
            let off = run(&store, (width, height), station, None, !off_only);
            write_png(
                &dir.join(format!("{}-off.png", station.name)),
                width,
                height,
                &off.rgba,
            );
            if let Some(d) = off.digest {
                let _ = writeln!(digests, "{} {d:016x}", station.name);
            }
            let _ = writeln!(counts, "{} {}", station.name, off.counts);
            if let Some(base) = &baseline {
                let base = base.join(backend);
                let same = read_png(&base.join(format!("{}-off.png", station.name)))
                    .map(|(_, _, b)| moved(&b, &off.rgba));
                let digest = std::fs::read_to_string(base.join("digest.txt"))
                    .ok()
                    .and_then(|t| {
                        t.lines().find_map(|l| {
                            l.strip_prefix(&format!("{} ", station.name))
                                .map(|d| d.trim().to_owned())
                        })
                    });
                eprintln!(
                    "{}: against the baseline, {} bytes differ; digest {}",
                    station.name,
                    same.map_or_else(|| "(no baseline frame)".into(), |n| n.to_string()),
                    match (digest, off.digest) {
                        (Some(a), Some(b)) if a == format!("{b:016x}") => "the same".to_owned(),
                        (Some(a), Some(b)) => format!("differs ({a} against {b:016x})"),
                        _ => "not compared".to_owned(),
                    }
                );
            }
            if off_only {
                continue;
            }
            let on = run(&store, (width, height), station, Some(&feature), true);
            write_png(
                &dir.join(format!("{}-on.png", station.name)),
                width,
                height,
                &on.rgba,
            );
            let changed = moved(&off.rgba, &on.rgba);
            eprintln!(
                "{} {feature}: {changed} bytes differ from off; cpu {:.3} ms off, {:.3} ms on",
                station.name,
                off.cpu_ms.unwrap_or(0.0),
                on.cpu_ms.unwrap_or(0.0)
            );
            let total: f32 = on.gpu_ms.iter().map(|(_, ms)| ms).sum();
            let passes: Vec<String> = on
                .gpu_ms
                .iter()
                .map(|(n, ms)| format!("{n}={ms:.4}"))
                .collect();
            let opt = |v: Option<f64>| v.map_or_else(String::new, |v| format!("{v:.4}"));
            let _ = writeln!(
                csv,
                "{},{backend},\"{adapter}\",,{width}x{height},{},{},{},{},{total:.4},\"{}\"",
                station.name,
                opt(off.cpu_ms),
                opt(on.cpu_ms),
                opt(off.cpu_ms_4k),
                opt(on.cpu_ms_4k),
                passes.join(" ")
            );
            for view in VIEWS
                .iter()
                .filter(|(f, _)| *f == feature)
                .flat_map(|(_, v)| v.iter())
            {
                let shown = run(&store, (width, height), station, Some(view), false);
                write_png(
                    &dir.join(format!("{}-{view}.png", station.name)),
                    width,
                    height,
                    &shown.rgba,
                );
            }
            sheet.push((off.rgba, on.rgba));
        }
        std::fs::write(dir.join("adapter.txt"), format!("{adapter}\n{backend}\n"))
            .expect("adapter.txt");
        std::fs::write(dir.join("stations.txt"), stations_txt).expect("stations.txt");
        if !digests.is_empty() {
            std::fs::write(dir.join("digest.txt"), digests).expect("digest.txt");
        }
        std::fs::write(dir.join("counters.txt"), counts).expect("counters.txt");
        if !off_only {
            std::fs::write(dir.join("timing.csv"), csv).expect("timing.csv");
            write_sheet(&dir.join("sheet.png"), &sheet, width, height);
        }
        eprintln!("wrote {}", dir.display());
    }
}

/// A short walk: each station in `DERETH_HIFI_STATIONS` drawn with the first feature of
/// `DERETH_HIFI_CAPTURE` (or off, with `off`), its viewer moved `DERETH_HIFI_WALK_STEP` metres
/// (default 1.5) along its heading between frames, `DERETH_HIFI_WALK_FRAMES` frames (default 12),
/// written as `<root>/walk-<feature>/<station>-NN.png` with a strip of them all, so what the
/// ground does round a moving viewer can be seen frame to frame.
#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_HIFI_CAPTURE_DIR set and --ignored"]
fn capture_a_walk() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (width, height) = size();
    let feature = features()
        .into_iter()
        .next()
        .unwrap_or_else(|| "off".into());
    let frames: usize = env("DERETH_HIFI_WALK_FRAMES")
        .and_then(|v| v.parse().ok())
        .unwrap_or(12);
    let step: f32 = env("DERETH_HIFI_WALK_STEP")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.5);
    // `DERETH_HIFI_WALK_EVERY` frames between captures (default 3); `DERETH_HIFI_WALK_SMOOTH=1`
    // spreads each step over them; `DERETH_HIFI_WALK_STOP` the capture the viewer stops at.
    let every: usize = env("DERETH_HIFI_WALK_EVERY")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3)
        .max(1);
    let smooth = env("DERETH_HIFI_WALK_SMOOTH").as_deref() == Some("1");
    let stop: usize = env("DERETH_HIFI_WALK_STOP")
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX / 4);
    let stations: Vec<Station> = hifi_stations::stations(&store)
        .into_iter()
        .chain(hifi_stations::close_stations(&store))
        .filter(wanted)
        .collect();
    let dir = root().join(format!("walk-{feature}"));
    std::fs::create_dir_all(&dir).expect("the capture folder");
    for station in &stations {
        let mut device = device(width, height);
        let gpu = &mut device;
        let mut shot = Shot::open(&store, gpu, station);
        #[cfg(feature = "hifi")]
        if let Some(prefs) = (feature != "off").then(|| fidelity_for(&feature)).flatten() {
            shot.scene.draw.cfg.render.fidelity = prefs;
        }
        shot.poll(gpu);
        // `DERETH_HIFI_WALK_FOLLOW=back,up,pitch-degrees`: a camera held behind a body.
        if let (Some(f), super::hifi_stations::Place::Ground { yaw, .. }) =
            (env("DERETH_HIFI_WALK_FOLLOW"), station.place)
        {
            let v: Vec<f32> = f.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if v.len() == 3 {
                shot.follow = Some([v[0], v[1], v[2].to_radians(), yaw.to_radians()]);
            }
        }
        let warm = env("DERETH_HIFI_WARM")
            .and_then(|w| w.parse().ok())
            .unwrap_or(WARM);
        for _ in 1..warm {
            shot.step(gpu);
            settle(gpu);
        }
        let mut row = Vec::new();
        for k in 0..frames {
            // Smooth: the viewer moves a share of the step on every frame between captures, as a
            // walker does, rather than jumping a whole step and standing.
            for s in 0..every {
                let f = k * every + s;
                #[allow(clippy::cast_precision_loss)] // a few frames
                let metres = if smooth {
                    f.min(stop.saturating_mul(every)) as f32 * step / every as f32
                } else {
                    k.min(stop) as f32 * step
                };
                if smooth || s == 0 {
                    shot.walk_to(station, metres);
                }
                shot.step(gpu);
            }
            let rgba = capture(gpu);
            write_png(
                &dir.join(format!("{}-{k:02}.png", station.name)),
                width,
                height,
                &rgba,
            );
            row.push(rgba);
        }
        let rows: Vec<Vec<Vec<u8>>> = row.chunks(4).map(<[Vec<u8>]>::to_vec).collect();
        write_grid(
            &dir.join(format!("{}-strip.png", station.name)),
            &rows,
            width,
            height,
        );
        eprintln!("{}: {frames} frames", station.name);
    }
}
