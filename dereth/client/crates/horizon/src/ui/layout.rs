//! The HUD layout: where the player has moved each HUD element and how large they have made it,
//! and the editor that changes it.
//!
//! Every element draws at its default place. The layout then moves and scales what it drew, about
//! the element's own top-left corner, and hands the element the pointer in its own frame while it
//! draws, so its buttons still answer where they are shown. An element's outline in the editor is
//! the box around what it drew.
//!
//! A layout is kept per character in a small text file in the client's settings folder: one line
//! per element, `name dx dy scale`. What the editor changes shows at once and is kept only when
//! the player saves it; closing the editor with changes unsaved puts back the layout last kept.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::art::Family;
use crate::draw::{Rect, WHITE};
use crate::ui::kit::Ctx;
use crate::ui::paint::{Painter, TextStyle};

/// How far a span from `at`, `len` long, must move to lie within `0..screen`; a span longer than
/// the screen keeps its start on it.
fn on_screen(at: f32, len: f32, screen: f32) -> f32 {
    if at < 0.0 || len >= screen {
        -at
    } else if at + len > screen {
        screen - (at + len)
    } else {
        0.0
    }
}

/// The smallest and largest an element may be made.
pub const SCALE_RANGE: (f32, f32) = (0.6, 2.0);

/// Where one element is: moved by `(dx, dy)` screen pixels at the reference height of 1080 lines,
/// and scaled about its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
        }
    }
}

impl Placement {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// An element being drawn: where its quads begin in the list, and the pointer it was lent.
#[derive(Debug)]
pub struct Open {
    name: &'static str,
    first_quad: usize,
    mouse: (f32, f32),
}

/// The layout and its editor.
#[derive(Debug, Default)]
pub struct HudLayout {
    pub placements: BTreeMap<String, Placement>,
    /// Each element's outline this frame, after its placement.
    pub outlines: Vec<(&'static str, Rect)>,
    /// The element the editor has selected, and where in it the drag took hold.
    pub selected: Option<&'static str>,
    grab: Option<(f32, f32)>,
    /// Each element's top-left corner as it drew itself, before its placement: what it is moved
    /// and scaled about.
    pivots: std::collections::HashMap<&'static str, (f32, f32)>,
    /// How far each element was moved, after its placement, to keep it on the screen.
    nudges: std::collections::HashMap<&'static str, (f32, f32)>,
    /// The file the layout was loaded from and is saved to.
    pub file: Option<PathBuf>,
    /// The placements as last loaded or saved.
    saved: BTreeMap<String, Placement>,
}

impl HudLayout {
    /// The layout in `file`, or the default layout when the file is absent or unreadable.
    #[must_use]
    pub fn load(file: PathBuf) -> Self {
        let mut layout = Self {
            file: Some(file.clone()),
            ..Self::default()
        };
        if let Ok(text) = std::fs::read_to_string(&file) {
            layout.placements = parse(&text);
        }
        layout.saved = layout.placements.clone();
        layout
    }

    /// Whether the placements differ from those last loaded or saved: an element put back where
    /// it started is no change.
    #[must_use]
    pub fn unsaved(&self) -> bool {
        let moved = |m: &BTreeMap<String, Placement>| {
            m.iter()
                .filter(|(_, p)| !p.is_default())
                .map(|(n, p)| (n.clone(), *p))
                .collect::<Vec<_>>()
        };
        moved(&self.placements) != moved(&self.saved)
    }

    /// Put back the placements last loaded or saved.
    pub fn revert(&mut self) {
        self.placements = self.saved.clone();
    }

    /// Write the layout to its file.
    ///
    /// # Errors
    /// When the file cannot be written.
    pub fn save(&mut self) -> std::io::Result<()> {
        if let Some(file) = &self.file {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(file, render(&self.placements))?;
        }
        self.saved = self.placements.clone();
        Ok(())
    }

    /// Put every element back where it started.
    pub fn reset(&mut self) {
        self.placements.clear();
    }

    fn placement(&self, name: &str) -> Placement {
        self.placements.get(name).copied().unwrap_or_default()
    }

    /// Start drawing element `name`: lend it the pointer in its own frame. While `editing`, the
    /// element sees no pointer at all.
    pub fn begin(
        &self,
        name: &'static str,
        p: &Painter<'_>,
        ctx: &mut Ctx<'_>,
        editing: bool,
    ) -> Open {
        let open = Open {
            name,
            first_quad: p.list.quads.len(),
            mouse: ctx.input.mouse,
        };
        if editing {
            ctx.input.mouse = (-10_000.0, -10_000.0);
        } else if let Some(pivot) = self.pivot(name) {
            let pl = self.placement(name);
            let k = p.scale;
            let (nx, ny) = self.nudge(name);
            let (mx, my) = ctx.input.mouse;
            let (mx, my) = (mx - nx, my - ny);
            ctx.input.mouse = (
                pivot.0 + (mx - pivot.0 - pl.dx * k) / pl.scale,
                pivot.1 + (my - pivot.1 - pl.dy * k) / pl.scale,
            );
        }
        open
    }

    /// Finish drawing an element: move and scale what it drew, note its outline, and give the
    /// pointer back.
    pub fn end(&mut self, open: Open, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
        ctx.input.mouse = open.mouse;
        let quads = &mut p.list.quads[open.first_quad..];
        let Some(mut bounds) = quads.first().map(|q| q.dst) else {
            return;
        };
        for q in quads.iter() {
            let (x0, y0) = (bounds.x.min(q.dst.x), bounds.y.min(q.dst.y));
            let (x1, y1) = (
                bounds.right().max(q.dst.right()),
                bounds.bottom().max(q.dst.bottom()),
            );
            bounds = Rect::new(x0, y0, x1 - x0, y1 - y0);
        }
        let pl = self.placement(open.name);
        let k = p.scale;
        let pivot = (bounds.x, bounds.y);
        let map = |r: Rect| {
            Rect::new(
                pivot.0 + (r.x - pivot.0) * pl.scale + pl.dx * k,
                pivot.1 + (r.y - pivot.1) * pl.scale + pl.dy * k,
                r.w * pl.scale,
                r.h * pl.scale,
            )
        };
        // Whatever the placement and the scale, the element stays on the screen.
        let placed = map(bounds);
        let nudge = (
            on_screen(placed.x, placed.w, p.screen.0),
            on_screen(placed.y, placed.h, p.screen.1),
        );
        let moved = |r: Rect| map(r).offset(nudge.0, nudge.1);
        if !pl.is_default() || nudge != (0.0, 0.0) {
            for q in quads.iter_mut() {
                q.dst = moved(q.dst);
                q.clip = q.clip.map(moved);
            }
        }
        self.pivots.insert(open.name, pivot);
        self.nudges.insert(open.name, nudge);
        self.outlines.push((open.name, moved(bounds)));
    }

    fn nudge(&self, name: &str) -> (f32, f32) {
        self.nudges.get(name).copied().unwrap_or((0.0, 0.0))
    }

    /// Where a rectangle element `name` drew is shown, after its placement.
    #[must_use]
    pub fn map_rect(&self, name: &str, r: Rect, k: f32) -> Rect {
        let pl = self.placement(name);
        let Some(pivot) = self.pivot(name) else {
            return r;
        };
        let (nx, ny) = self.nudge(name);
        Rect::new(
            pivot.0 + (r.x - pivot.0) * pl.scale + pl.dx * k + nx,
            pivot.1 + (r.y - pivot.1) * pl.scale + pl.dy * k + ny,
            r.w * pl.scale,
            r.h * pl.scale,
        )
    }

    fn pivot(&self, name: &str) -> Option<(f32, f32)> {
        self.pivots.get(name).copied()
    }

    /// The editor, over the HUD: every element outlined and named, the selected one gold; drag
    /// to move, the wheel to resize.
    pub fn edit(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
        let k = p.scale;
        let label = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let outlines = std::mem::take(&mut self.outlines);
        // Drag the selected element while the button is held.
        if let (Some(name), Some((gx, gy))) = (self.selected, self.grab) {
            if ctx.input.down[0] {
                if let Some((_, r)) = outlines.iter().find(|(n, _)| *n == name) {
                    let pl = self.placements.entry(name.to_owned()).or_default();
                    pl.dx += (ctx.input.mouse.0 - gx - r.x) / k;
                    pl.dy += (ctx.input.mouse.1 - gy - r.y) / k;
                }
                ctx.input.captured = true;
            } else {
                self.grab = None;
            }
        }
        for (name, r) in &outlines {
            let selected = self.selected == Some(*name);
            let over = ctx.input.hover(r);
            let fill = if selected {
                0x40FF_C040
            } else if over {
                0x30FF_FFFF
            } else {
                0x20FF_FFFF
            };
            p.fill(*r, fill);
            let edge = if selected { 0xFFFF_C040 } else { 0xC0FF_FFFF };
            for side in [
                Rect::new(r.x, r.y, r.w, 1.5),
                Rect::new(r.x, r.bottom() - 1.5, r.w, 1.5),
                Rect::new(r.x, r.y, 1.5, r.h),
                Rect::new(r.right() - 1.5, r.y, 1.5, r.h),
            ] {
                p.fill(side, edge);
            }
            p.text(&label, r.x + 4.0 * k, r.y + 2.0 * k, &element_title(name));
            if over && ctx.input.pressed[0] && !ctx.input.captured {
                self.selected = Some(name);
                self.grab = Some((ctx.input.mouse.0 - r.x, ctx.input.mouse.1 - r.y));
                ctx.input.pressed[0] = false;
                ctx.input.captured = true;
            }
            if selected && over && ctx.input.wheel != 0.0 {
                let pl = self.placements.entry((*name).to_owned()).or_default();
                let step = if ctx.input.wheel > 0.0 { 0.1 } else { -0.1 };
                pl.scale = (pl.scale + step).clamp(SCALE_RANGE.0, SCALE_RANGE.1);
                ctx.input.wheel = 0.0;
            }
        }
        let _ = WHITE;
        self.outlines = outlines;
    }

    /// The selected element's placement, for the layout window.
    #[must_use]
    pub fn selected_placement(&self) -> Option<(&'static str, Placement)> {
        self.selected.map(|n| (n, self.placement(n)))
    }
}

/// An element's name as the editor labels it.
#[must_use]
pub fn element_title(name: &str) -> String {
    match name {
        "parameter" => "Parameter Bar",
        "stance" => "Stance Gauge",
        "hotbars" => "Hotbars",
        "exp" => "EXP Bar",
        "target" => "Target Info",
        "status" => "Status Info",
        "minimap" => "Minimap",
        "server" => "Server Info",
        "party" => "Party List",
        "duty" => "Duty List",
        "chat" => "Log Window",
        "gil" => "Currency",
        "menu" => "Main Menu",
        other => other,
    }
    .to_owned()
}

/// A layout file's text.
#[must_use]
pub fn render(placements: &BTreeMap<String, Placement>) -> String {
    let mut out = String::new();
    for (name, p) in placements.iter().filter(|(_, p)| !p.is_default()) {
        out.push_str(&format!("{name} {:.1} {:.1} {:.2}\n", p.dx, p.dy, p.scale));
    }
    out
}

/// Read a layout file's text. A line that does not read is skipped.
#[must_use]
pub fn parse(text: &str) -> BTreeMap<String, Placement> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let [name, dx, dy, scale] = parts[..] {
            if let (Ok(dx), Ok(dy), Ok(scale)) = (dx.parse(), dy.parse(), scale.parse::<f32>()) {
                out.insert(
                    name.to_owned(),
                    Placement {
                        dx,
                        dy,
                        scale: scale.clamp(SCALE_RANGE.0, SCALE_RANGE.1),
                    },
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: none (the layout file's own format)
    #[test]
    fn a_layout_reads_back_what_it_wrote_and_leaves_out_untouched_elements() {
        let mut placements = BTreeMap::new();
        placements.insert(
            "minimap".to_owned(),
            Placement {
                dx: -40.0,
                dy: 12.5,
                scale: 1.2,
            },
        );
        placements.insert("chat".to_owned(), Placement::default());
        let text = render(&placements);
        assert_eq!(text, "minimap -40.0 12.5 1.20\n");
        let back = parse(&text);
        assert_eq!(back.len(), 1);
        assert_eq!(back["minimap"].scale, 1.2);
    }

    /// Behaviour: none (the layout file's own format)
    #[test]
    fn a_scale_out_of_range_is_brought_inside_it_and_a_bad_line_is_skipped() {
        let back = parse("hotbars 0 0 9\nnonsense\nexp 1 2\n");
        assert_eq!(back.len(), 1);
        assert_eq!(back["hotbars"].scale, SCALE_RANGE.1);
    }

    /// Behaviour: none (experimental Horizon interface)
    #[test]
    fn an_element_scaled_past_the_screen_s_edge_is_moved_back_onto_it() {
        assert_eq!(on_screen(10.0, 50.0, 100.0), 0.0);
        assert_eq!(on_screen(80.0, 50.0, 100.0), -30.0);
        assert_eq!(on_screen(-20.0, 50.0, 100.0), 20.0);
        assert_eq!(
            on_screen(30.0, 150.0, 100.0),
            -30.0,
            "too long: its start on the screen"
        );
    }
}
