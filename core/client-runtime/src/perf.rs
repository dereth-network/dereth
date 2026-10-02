//! The performance panel: the frame rate, the frame time and where the frame's time goes, drawn
//! over the game in the top left corner by the runtime itself, whatever the interface.
//!
//! It is shown while [`PERFORMANCE_PANEL`] is on, which the option and the
//! [`TOGGLE_PERFORMANCE_PANEL`] action set. [`FramePerf`] keeps the last second of frames; the
//! panel is drawn through the presentation's overlay with a small font of its own, so it needs
//! no interface's files and draws the same on every interface, headless or on the web.
//!
//! [`PERFORMANCE_PANEL`]: dereth_client_contract::options::performance::PERFORMANCE_PANEL
//! [`TOGGLE_PERFORMANCE_PANEL`]: dereth_client_contract::actions::dereth::TOGGLE_PERFORMANCE_PANEL

use dereth_client_contract::overlay::{
    OverlayItem, OverlayMaterial, OverlaySampler, OverlaySpace, OverlayTexture, OverlayVertex,
};
use dereth_primitives::{TextureData, TextureFormat};

use crate::frame::{FrameStep, STEPS};

/// How many frames the averages are over at most.
pub const WINDOW: usize = 240;

/// The parts of a frame the panel names, each a run of [`FrameStep`]s.
pub const PHASES: [(&str, &[FrameStep]); 5] = [
    (
        "INPUT/NET",
        &[
            FrameStep::UiQueueStep,
            FrameStep::ClockSample,
            FrameStep::ProcessWindowEvents,
            FrameStep::NetworkStep,
            FrameStep::LoginEvents,
            FrameStep::PacketStep,
            FrameStep::AssetCacheStep,
        ],
    ),
    ("UI", &[FrameStep::UiStep]),
    ("WORLD", &[FrameStep::WorldViewStep]),
    (
        "DRAW",
        &[
            FrameStep::PrepareDevice,
            FrameStep::BeginFrame,
            FrameStep::DrawWorld,
            FrameStep::PresentFrame,
        ],
    ),
    ("PACE", &[FrameStep::PaceFrame]),
];

/// One frame as the panel counts it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sample {
    /// Seconds from the start of the frame before to the start of this one.
    interval: f32,
    /// Seconds in each step of the frame before.
    steps: [f32; STEPS],
}

/// The last frames' times.
#[derive(Debug, Default)]
pub struct FramePerf {
    samples: std::collections::VecDeque<Sample>,
    last_start: Option<web_time::Instant>,
}

/// What the panel shows, worked out from [`FramePerf`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PerfSummary {
    /// Frames a second, over the frames kept.
    pub fps: f32,
    /// The mean frame time, in milliseconds.
    pub mean_ms: f32,
    /// The longest frame time kept, in milliseconds.
    pub max_ms: f32,
    /// The mean milliseconds of each of [`PHASES`].
    pub phases_ms: [f32; PHASES.len()],
    /// How many frames the figures are over.
    pub frames: usize,
}

impl FramePerf {
    /// A frame is starting at `now`, and the one before it spent `steps` in its steps.
    pub fn frame_started(&mut self, now: web_time::Instant, steps: [f32; STEPS]) {
        if let Some(last) = self.last_start.replace(now) {
            self.push(now.duration_since(last).as_secs_f32(), steps);
        }
    }

    /// Record one frame of `interval` seconds.
    pub fn push(&mut self, interval: f32, steps: [f32; STEPS]) {
        if self.samples.len() == WINDOW {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample { interval, steps });
    }

    /// The figures over the frames kept.
    #[must_use]
    pub fn summary(&self) -> PerfSummary {
        let n = self.samples.len();
        if n == 0 {
            return PerfSummary::default();
        }
        let total: f32 = self.samples.iter().map(|s| s.interval).sum();
        let max = self.samples.iter().map(|s| s.interval).fold(0.0, f32::max);
        let mut phases_ms = [0.0; PHASES.len()];
        for (slot, (_, steps)) in phases_ms.iter_mut().zip(PHASES) {
            let sum: f32 = self
                .samples
                .iter()
                .map(|s| steps.iter().map(|st| s.steps[st.index()]).sum::<f32>())
                .sum();
            #[allow(clippy::cast_precision_loss)]
            {
                *slot = sum / n as f32 * 1000.0;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let mean = total / n as f32;
        PerfSummary {
            fps: if total > 0.0 { 1.0 / mean } else { 0.0 },
            mean_ms: mean * 1000.0,
            max_ms: max * 1000.0,
            phases_ms,
            frames: n,
        }
    }
}

impl PerfSummary {
    /// The panel's lines of text.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![
            format!("FPS {:6.1}", self.fps),
            format!("FRAME {:6.2} MS", self.mean_ms),
            format!("MAX   {:6.2} MS", self.max_ms),
        ];
        for ((name, _), ms) in PHASES.iter().zip(self.phases_ms) {
            out.push(format!("{name:<9} {ms:6.2}"));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------

/// The panel's font sheet: every glyph of [`GLYPHS`] in one row, [`CELL_W`] by [`CELL_H`] each.
pub const FONT_TEXTURE: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x5045_5246_464F_4E54,
};

/// One glyph's cell in the font sheet, including its blank column and row.
pub const CELL_W: u32 = 6;
pub const CELL_H: u32 = 8;

/// How many screen pixels a font pixel covers.
pub const SCALE: f32 = 2.0;

/// A 5 by 7 font: each glyph is seven rows, the top one first, each row's five pixels in the low
/// five bits, the leftmost in the highest of them. Unknown characters draw as a blank.
pub const GLYPHS: &[(char, [u8; 7])] = &[
    (' ', [0, 0, 0, 0, 0, 0, 0]),
    ('0', [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E]),
    ('1', [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E]),
    ('2', [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F]),
    ('3', [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E]),
    ('4', [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02]),
    ('5', [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E]),
    ('6', [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E]),
    ('7', [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08]),
    ('8', [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E]),
    ('9', [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C]),
    ('.', [0, 0, 0, 0, 0, 0x0C, 0x0C]),
    ('/', [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10]),
    ('-', [0, 0, 0, 0x1F, 0, 0, 0]),
    ('A', [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]),
    ('C', [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E]),
    ('D', [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E]),
    ('E', [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F]),
    ('F', [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10]),
    ('I', [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E]),
    ('L', [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F]),
    ('M', [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11]),
    ('N', [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11]),
    ('O', [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
    ('P', [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10]),
    ('R', [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11]),
    ('S', [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E]),
    ('T', [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04]),
    ('U', [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
    ('W', [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A]),
    ('X', [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11]),
];

/// The font sheet's pixels: white where a glyph is drawn, clear elsewhere.
#[must_use]
pub fn font_sheet() -> TextureData {
    #[allow(clippy::cast_possible_truncation)]
    let width = CELL_W * GLYPHS.len() as u32;
    let mut px = vec![0u8; (width * CELL_H * 4) as usize];
    for (i, (_, rows)) in GLYPHS.iter().enumerate() {
        for (y, row) in rows.iter().enumerate() {
            for x in 0..5u32 {
                if row & (0x10 >> x) != 0 {
                    #[allow(clippy::cast_possible_truncation)]
                    let at = ((y as u32 * width + i as u32 * CELL_W + x) * 4) as usize;
                    px[at..at + 4].copy_from_slice(&[0xFF; 4]);
                }
            }
        }
    }
    TextureData {
        width,
        height: CELL_H,
        format: TextureFormat::Bgra8,
        levels: vec![px],
    }
}

/// The panel's backdrop colour and its text colour, `0xAARRGGBB`.
pub const BACKDROP: u32 = 0xB000_0000;
pub const TEXT: u32 = 0xFF7F_FF7F;

/// The panel as overlay triangles for a back buffer of `size` pixels: a dark backdrop, then the
/// lines. The backdrop is drawn from the font sheet's blank, so the panel needs one texture.
#[must_use]
pub fn panel_items(lines: &[String], size: (u32, u32)) -> Vec<OverlayItem> {
    #[allow(clippy::cast_precision_loss)]
    let (sw, sh) = (size.0.max(1) as f32, size.1.max(1) as f32);
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let sheet_w = (CELL_W * GLYPHS.len() as u32) as f32;
    #[allow(clippy::cast_precision_loss)]
    let (cw, ch) = (CELL_W as f32 * SCALE, CELL_H as f32 * SCALE);
    let pad = 4.0;
    let cols = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (
        cols as f32 * cw + pad * 2.0,
        lines.len() as f32 * ch + pad * 2.0,
    );
    // Below the top edge, where the interfaces keep bars of their own.
    let (x0, y0) = (4.0, 40.0);
    let mut v = Vec::new();
    // Pixels to clip space, y up.
    let to_clip = |x: f32, y: f32| [x / sw * 2.0 - 1.0, 1.0 - y / sh * 2.0, 0.5];
    let mut quad = |x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], color: u32| {
        let corners = [
            (x, y, uv[0], uv[1]),
            (x, y + h, uv[0], uv[3]),
            (x + w, y + h, uv[2], uv[3]),
            (x + w, y + h, uv[2], uv[3]),
            (x + w, y, uv[2], uv[1]),
            (x, y, uv[0], uv[1]),
        ];
        for (px, py, u, t) in corners {
            v.push(OverlayVertex {
                position: to_clip(px, py),
                color,
                uv: [u, t],
            });
        }
    };
    // The blank cell's middle, stretched: an opaque white texel times the backdrop colour would
    // need a second texture; the space glyph's cell is clear, so the backdrop takes the colour from
    // a lit pixel of the '.' glyph instead.
    let dot = GLYPHS.iter().position(|(c, _)| *c == '.').unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    let solid_u = (dot as f32 * CELL_W as f32 + 2.5) / sheet_w;
    let solid_v = 5.5 / CELL_H as f32;
    quad(x0, y0, w, h, [solid_u, solid_v, solid_u, solid_v], BACKDROP);
    for (row, line) in lines.iter().enumerate() {
        for (col, c) in line.chars().enumerate() {
            let Some(i) = GLYPHS.iter().position(|(g, _)| *g == c) else {
                continue;
            };
            if c == ' ' {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let u0 = i as f32 * CELL_W as f32 / sheet_w;
            #[allow(clippy::cast_precision_loss)]
            let u1 = (i as f32 * CELL_W as f32 + CELL_W as f32) / sheet_w;
            #[allow(clippy::cast_precision_loss)]
            quad(
                x0 + pad + col as f32 * cw,
                y0 + pad + row as f32 * ch,
                cw,
                ch,
                [u0, 0.0, u1, 1.0],
                TEXT,
            );
        }
    }
    vec![OverlayItem::Triangles {
        material: OverlayMaterial::Image,
        texture: FONT_TEXTURE,
        sampler: OverlaySampler::POINT_CLAMP,
        vertices: v,
    }]
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the panel's own arithmetic and font; the panel on screen is the live
    //! client's, captured in the work's notes).
    use super::*;

    #[test]
    fn the_summary_averages_the_frames_and_names_the_longest() {
        let mut p = FramePerf::default();
        let mut steps = [0.0; STEPS];
        steps[FrameStep::DrawWorld.index()] = 0.004;
        steps[FrameStep::PresentFrame.index()] = 0.002;
        steps[FrameStep::UiStep.index()] = 0.001;
        p.push(0.010, steps);
        p.push(0.030, steps);
        let s = p.summary();
        assert_eq!(s.frames, 2);
        assert!((s.fps - 50.0).abs() < 0.01, "{s:?}");
        assert!((s.mean_ms - 20.0).abs() < 0.01);
        assert!((s.max_ms - 30.0).abs() < 0.01);
        assert!((s.phases_ms[3] - 6.0).abs() < 0.01, "DRAW");
        assert!((s.phases_ms[1] - 1.0).abs() < 0.01, "UI");
        for _ in 0..WINDOW {
            p.push(0.016, steps);
        }
        assert_eq!(p.summary().frames, WINDOW);
    }

    #[test]
    fn every_character_the_panel_writes_has_a_glyph() {
        let s = PerfSummary {
            fps: 59.9,
            mean_ms: 16.7,
            max_ms: 33.3,
            phases_ms: [0.5, 1.0, 2.0, 3.0, 10.0],
            frames: 60,
        };
        for line in s.lines() {
            for c in line.chars() {
                assert!(GLYPHS.iter().any(|(g, _)| *g == c), "{c:?} in {line:?}");
            }
        }
        let sheet = font_sheet();
        assert_eq!(
            sheet.levels[0].len(),
            (sheet.width * sheet.height * 4) as usize
        );
        let items = panel_items(&s.lines(), (800, 600));
        let OverlayItem::Triangles { vertices, .. } = &items[0] else {
            panic!("triangles");
        };
        assert_eq!(vertices.len() % 6, 0);
        assert!(vertices
            .iter()
            .all(|v| v.position[0] >= -1.0 && v.position[1] <= 1.0));
    }
}
