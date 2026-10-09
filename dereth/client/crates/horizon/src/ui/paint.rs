//! Drawing the interface's pieces: sprites, nine-grids, gauges and text in the game's fonts, in
//! layout units scaled to the screen.

use std::sync::Arc;

use crate::art::{Art, Face, Family, Sprite};
use crate::draw::{with_alpha, Argb, DrawList, Rect};

/// Where a line of text sits in its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Centre,
    Right,
}

/// A text style: the family, its size in points, its colour and its edge (outline) colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub family: Family,
    pub points: f32,
    pub colour: Argb,
    pub edge: Option<Argb>,
    /// Extra space after every glyph, in layout units.
    pub tracking: f32,
}

impl TextStyle {
    #[must_use]
    pub const fn new(family: Family, points: f32, colour: Argb) -> Self {
        Self {
            family,
            points,
            colour,
            edge: None,
            tracking: 0.0,
        }
    }

    #[must_use]
    pub const fn tracking(mut self, units: f32) -> Self {
        self.tracking = units;
        self
    }

    #[must_use]
    pub const fn edge(mut self, edge: Argb) -> Self {
        self.edge = Some(edge);
        self
    }

    #[must_use]
    pub const fn colour(mut self, colour: Argb) -> Self {
        self.colour = colour;
        self
    }
}

/// The painter: a draw list, the art it draws from and the layout scale.
#[derive(Debug)]
pub struct Painter<'a> {
    pub list: &'a mut DrawList,
    pub art: &'a Art,
    /// Screen pixels per layout unit.
    pub scale: f32,
    /// The screen's size in pixels.
    pub screen: (f32, f32),
    /// Multiplies the alpha of everything drawn (a window fading in).
    pub fade: f32,
}

impl Painter<'_> {
    fn a(&self, colour: Argb) -> Argb {
        if (self.fade - 1.0).abs() < f32::EPSILON {
            colour
        } else {
            with_alpha(colour, self.fade)
        }
    }

    /// A sprite stretched over `dst` (in screen pixels).
    pub fn sprite(&mut self, s: &Sprite, dst: Rect, colour: Argb) {
        let c = self.a(colour);
        self.list.image(s.tex, s.src, dst, c);
    }

    /// A sprite stretched over `dst`, turned `turn` radians clockwise about its centre.
    pub fn sprite_turned(&mut self, s: &Sprite, dst: Rect, colour: Argb, turn: f32) {
        let c = self.a(colour);
        self.list.image_turned(s.tex, s.src, dst, c, turn);
    }

    /// A sprite at its own size times the scale, top-left at `(x, y)` screen pixels.
    pub fn sprite_at(&mut self, s: &Sprite, x: f32, y: f32, colour: Argb) {
        let dst = Rect::new(x, y, s.w * self.scale, s.h * self.scale);
        self.sprite(s, dst, colour);
    }

    /// The interface's own piece `name`, sharp enough for the current scale.
    #[must_use]
    pub fn piece(&self, name: &str) -> Option<Sprite> {
        self.art.piece(name, self.scale)
    }

    /// `s` repeated over `dst` at its own size times `size_scale` screen pixels a unit, from the
    /// top-left, the last copies cut at `dst`'s edges.
    pub fn tiled(&mut self, s: &Sprite, dst: Rect, size_scale: f32, colour: Argb) {
        let (tw, th) = (s.w * size_scale, s.h * size_scale);
        if tw <= 0.5 || th <= 0.5 || dst.w <= 0.0 || dst.h <= 0.0 {
            return;
        }
        let mut y = dst.y;
        while y < dst.bottom() - 0.01 {
            let h = th.min(dst.bottom() - y);
            let mut x = dst.x;
            while x < dst.right() - 0.01 {
                let w = tw.min(dst.right() - x);
                let part = s.sub(0.0, 0.0, w / size_scale, h / size_scale);
                self.sprite(&part, Rect::new(x, y, w, h), colour);
                x += tw;
            }
            y += th;
        }
    }

    /// A stretchable plate of the interface's own art, `group.left`, `group.mid` and
    /// `group.right`, drawn over `dst` at `dst`'s height: the left half from the left, the right
    /// half from the right, each cut to fit, and the centre tile repeated between them only when
    /// the halves fall short. Whether the pieces were there to draw.
    pub fn halves(&mut self, group: &str, dst: Rect, colour: Argb) -> bool {
        let at = |p: &Self, part: &str| {
            let probe = p.art.piece(&format!("{group}.{part}"), 1.0)?;
            let k = dst.h / probe.h.max(f32::EPSILON);
            p.art.piece(&format!("{group}.{part}"), k).map(|s| (s, k))
        };
        let (Some((left, k)), Some((right, _)), Some((mid, _))) =
            (at(self, "left"), at(self, "right"), at(self, "mid"))
        else {
            return false;
        };
        let lw = (left.w * k).min((dst.w / 2.0).floor());
        let rw = (right.w * k).min(dst.w - lw);
        self.sprite(
            &left.sub(0.0, 0.0, lw / k, left.h),
            Rect::new(dst.x, dst.y, lw, dst.h),
            colour,
        );
        self.sprite(
            &right.sub(right.w - rw / k, 0.0, rw / k, right.h),
            Rect::new(dst.right() - rw, dst.y, rw, dst.h),
            colour,
        );
        let gap = Rect::new(dst.x + lw, dst.y, dst.w - lw - rw, dst.h);
        if gap.w > 0.5 {
            self.tiled(&mid, gap, k, colour);
        }
        true
    }

    /// A frame of the interface's own art over `dst`: `group.tl`, `.top`, `.tr`, `.left`,
    /// `.right`, `.bl`, `.bottom`, `.br`, the corners at their own size times the scale and the
    /// edges repeated along. Whether the pieces were there to draw.
    pub fn frame(&mut self, group: &str, top: &str, dst: Rect, colour: Argb) -> bool {
        let names = [
            format!("{group}.{top}tl"),
            format!("{group}.{top}top"),
            format!("{group}.{top}tr"),
            format!("{group}.left"),
            format!("{group}.right"),
            format!("{group}.bl"),
            format!("{group}.bottom"),
            format!("{group}.br"),
        ];
        let parts: Vec<Option<Sprite>> = names.iter().map(|n| self.piece(n)).collect();
        let Some(parts) = parts.into_iter().collect::<Option<Vec<Sprite>>>() else {
            return false;
        };
        let k = self.scale;
        let [tl, t, tr, l, r, bl, b, br] = [
            &parts[0], &parts[1], &parts[2], &parts[3], &parts[4], &parts[5], &parts[6], &parts[7],
        ];
        let (lw, rw) = (tl.w * k, tr.w * k);
        let (th, bh) = (tl.h * k, bl.h * k);
        self.tiled(
            t,
            Rect::new(dst.x + lw, dst.y, dst.w - lw - rw, th),
            k,
            colour,
        );
        self.tiled(
            b,
            Rect::new(
                dst.x + bl.w * k,
                dst.bottom() - bh,
                dst.w - bl.w * k - br.w * k,
                bh,
            ),
            k,
            colour,
        );
        self.tiled(
            l,
            Rect::new(dst.x, dst.y + th, l.w * k, dst.h - th - bh),
            k,
            colour,
        );
        self.tiled(
            r,
            Rect::new(dst.right() - r.w * k, dst.y + th, r.w * k, dst.h - th - bh),
            k,
            colour,
        );
        self.sprite(tl, Rect::new(dst.x, dst.y, lw, th), colour);
        self.sprite(tr, Rect::new(dst.right() - rw, dst.y, rw, tr.h * k), colour);
        self.sprite(
            bl,
            Rect::new(dst.x, dst.bottom() - bh, bl.w * k, bh),
            colour,
        );
        self.sprite(
            br,
            Rect::new(
                dst.right() - br.w * k,
                dst.bottom() - br.h * k,
                br.w * k,
                br.h * k,
            ),
            colour,
        );
        true
    }

    pub fn fill(&mut self, dst: Rect, colour: Argb) {
        let c = self.a(colour);
        self.list.fill(dst, c);
    }

    /// A frame of `width` around the inside of `dst`.
    pub fn outline(&mut self, dst: Rect, width: f32, colour: Argb) {
        self.fill(Rect::new(dst.x, dst.y, dst.w, width), colour);
        self.fill(Rect::new(dst.x, dst.bottom() - width, dst.w, width), colour);
        self.fill(Rect::new(dst.x, dst.y, width, dst.h), colour);
        self.fill(Rect::new(dst.right() - width, dst.y, width, dst.h), colour);
    }

    /// The face for `style`, at the size nearest its points times the scale.
    #[must_use]
    pub fn face(&self, style: &TextStyle) -> Option<Arc<Face>> {
        self.art.font(style.family, style.points * self.scale)
    }

    /// How much wider than its own pixels the face is drawn, to land on `style.points`.
    fn glyph_scale(&self, face: &Face, style: &TextStyle) -> f32 {
        let wanted = style.points * self.scale;
        if face.font.point_size > 0.0 {
            wanted / face.font.point_size
        } else {
            1.0
        }
    }

    /// The width of `text` in screen pixels.
    #[must_use]
    pub fn measure(&self, style: &TextStyle, text: &str) -> f32 {
        let Some(face) = self.face(style) else {
            return 0.0;
        };
        #[allow(clippy::cast_precision_loss)]
        let w = face.font.measure(text) as f32;
        #[allow(clippy::cast_precision_loss)]
        let n = text.chars().count() as f32;
        w * self.glyph_scale(&face, style) + style.tracking * self.scale * (n - 1.0).max(0.0)
    }

    /// The line height of `style` in screen pixels.
    #[must_use]
    pub fn line_height(&self, style: &TextStyle) -> f32 {
        let Some(face) = self.face(style) else {
            return style.points * self.scale * 1.4;
        };
        #[allow(clippy::cast_precision_loss)]
        let h = face.font.line_height as f32;
        h * self.glyph_scale(&face, style)
    }

    /// `text` with its top-left at `(x, y)` (screen pixels). Returns its width.
    pub fn text(&mut self, style: &TextStyle, x: f32, y: f32, text: &str) -> f32 {
        let Some(face) = self.face(style) else {
            return 0.0;
        };
        let k = self.glyph_scale(&face, style);
        if let Some(edge) = style.edge {
            let o = (k * 1.0).max(1.0);
            for (dx, dy) in [
                (-o, 0.0),
                (o, 0.0),
                (0.0, -o),
                (0.0, o),
                (-o, -o),
                (o, -o),
                (-o, o),
                (o, o),
            ] {
                self.glyphs(
                    &face,
                    k,
                    style.tracking * self.scale,
                    x + dx,
                    y + dy,
                    text,
                    edge,
                );
            }
        }
        self.glyphs(
            &face,
            k,
            style.tracking * self.scale,
            x,
            y,
            text,
            style.colour,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn glyphs(
        &mut self,
        face: &Face,
        k: f32,
        tracking: f32,
        x: f32,
        y: f32,
        text: &str,
        colour: Argb,
    ) -> f32 {
        let colour = self.a(colour);
        let mut pen = x;
        let mut prev = None;
        for ch in text.chars() {
            let Some(g) = face.font.glyph(ch).or_else(|| face.font.glyph('?')) else {
                continue;
            };
            if let Some(p) = prev {
                #[allow(clippy::cast_precision_loss)]
                let kern = face.font.kern(p, ch) as f32;
                pen += kern * k;
            }
            if g.w > 0 && g.h > 0 {
                if let Some(tex) = self.art.font_page(face, g.page) {
                    let src = Rect::new(
                        f32::from(g.x),
                        f32::from(g.y),
                        f32::from(g.w),
                        f32::from(g.h),
                    );
                    let dst = Rect::new(
                        pen,
                        y + f32::from(g.y_offset) * k,
                        f32::from(g.w) * k,
                        f32::from(g.h) * k,
                    );
                    self.list.image(tex, src, dst, colour);
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let adv = g.advance() as f32;
            pen += adv * k + tracking;
            prev = Some(ch);
        }
        pen - x - tracking
    }

    /// Where the letters of `style` sit in a line of it, as the eye weighs them: from midway
    /// between the capitals' top and the small letters' top down to the baseline, in screen pixels
    /// below the line's top. `None` when the face has no letters to measure.
    #[must_use]
    pub fn letter_band(&self, style: &TextStyle) -> Option<(f32, f32)> {
        let face = self.face(style)?;
        let cap = face.font.glyph('H')?;
        let small = face.font.glyph('x').unwrap_or(cap);
        let k = self.glyph_scale(&face, style);
        let top = (f32::from(cap.y_offset) + f32::from(small.y_offset)) / 2.0 * k;
        let base = (f32::from(cap.y_offset) + f32::from(cap.h)) * k;
        Some((top, base))
    }

    /// The top of a line of `style` whose letters sit in the middle of a box `y` down and `h`
    /// tall: the letters centred, not the whole line, whose room under the baseline for the
    /// descenders would otherwise leave them low.
    #[must_use]
    pub fn text_top(&self, style: &TextStyle, y: f32, h: f32) -> f32 {
        let top = match self.letter_band(style) {
            Some((top, base)) => y + (h - top - base) / 2.0,
            None => y + (h - self.line_height(style)) / 2.0,
        };
        top.round()
    }

    /// `text` placed in `r` by `align`, its letters vertically centred.
    pub fn text_in(&mut self, style: &TextStyle, r: Rect, align: Align, text: &str) -> f32 {
        let w = self.measure(style, text);
        let x = match align {
            Align::Left => r.x,
            Align::Centre => r.x + (r.w - w) / 2.0,
            Align::Right => r.right() - w,
        };
        let y = self.text_top(style, r.y, r.h);
        self.text(style, x.round(), y, text)
    }

    /// `text` cut with an ellipsis to fit `max_w` screen pixels.
    #[must_use]
    pub fn fit(&self, style: &TextStyle, text: &str, max_w: f32) -> String {
        if self.measure(style, text) <= max_w {
            return text.to_owned();
        }
        let mut s: String = text.to_owned();
        while !s.is_empty() && self.measure(style, &format!("{s}...")) > max_w {
            s.pop();
        }
        format!("{s}...")
    }

    /// `text` broken into lines no wider than `max_w`, at spaces.
    #[must_use]
    pub fn wrap(&self, style: &TextStyle, text: &str, max_w: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let candidate = if line.is_empty() {
                    word.to_owned()
                } else {
                    format!("{line} {word}")
                };
                if self.measure(style, &candidate) <= max_w || line.is_empty() {
                    line = candidate;
                } else {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_owned();
                }
            }
            lines.push(line);
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own text drawing)
    use super::*;

    /// Text placed in a box sits with its letters in the box's middle, not its whole line, whose
    /// room for descenders would leave the letters low: the box's middle falls between the
    /// middle of the capitals and the middle of the small letters.
    #[test]
    fn text_in_a_box_centres_its_letters() {
        let art = Art::new(std::sync::Arc::new(
            crate::pieces::Pieces::built_in().expect("the pieces built in"),
        ));
        let mut list = DrawList::default();
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        let style = TextStyle::new(Family::Body, 13.0, 0xFFFF_FFFF);
        let row = Rect::new(0.0, 100.0, 200.0, 30.0);
        let middle = row.y + row.h / 2.0;
        p.text_in(&style, row, Align::Left, "H");
        let cap = list.quads.last().expect("the capital was drawn").dst;
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        p.text_in(&style, row, Align::Left, "x");
        let small = list.quads.last().expect("the small letter was drawn").dst;
        let (cap_middle, small_middle) = (cap.y + cap.h / 2.0, small.y + small.h / 2.0);
        assert!(
            cap_middle <= middle + 0.6 && small_middle >= middle - 0.6,
            "the capital's middle is at {cap_middle}, the small letter's at {small_middle}, the              row's at {middle}"
        );
        assert!(
            small_middle - middle < 2.0,
            "the small letters sit low: {small_middle} against {middle}"
        );
    }
}
