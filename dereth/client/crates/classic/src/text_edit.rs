//! The classic edit field's glyph-based layout: word wrap, the nearest glyph to a point, the
//! caret's vertical motion between lines, keeping the caret visible, and the caret's blink.
//! Indices are Rust character indices; non-BMP input remains a host adapter.
//! Soft lines contain no sentinel: their final caret precedes their last glyph.

use crate::int::i32_from;
use crate::widgets::Rect;
use dereth_primitives::num::to_i32_f64;

/// The caret blinks by elapsed time, toggling at most once per dispatched timer event.
#[derive(Clone, Debug)]
pub struct CaretBlink {
    moved: f64,
    toggled: f64,
    pub visible: bool,
}
impl CaretBlink {
    pub fn new(now: f64) -> Self {
        Self {
            moved: now,
            toggled: now,
            visible: true,
        }
    }
    pub fn activity(&mut self, now: f64) {
        self.moved = now;
        self.visible = true;
    }
    pub fn tick(&mut self, now: f64) {
        if now - self.moved < 1.5 {
            self.visible = true;
        } else if now - self.toggled >= 1.0 {
            self.toggled = now;
            self.visible = !self.visible;
        }
    }
}

/// Smooth text scrolling, which an edit field enables with its flag 0x80.
/// Fractions accumulate independently of the displayed integer position.
#[derive(Clone, Debug)]
pub struct SmoothScroll {
    current: (f64, f64),
    target: (i32, i32),
    time: f64,
    speed: f64,
}
impl SmoothScroll {
    pub fn new(current: (i32, i32), target: (i32, i32), now: f64, speed: i32) -> Self {
        Self {
            current: (current.0 as f64, current.1 as f64),
            target,
            time: now,
            speed: speed.max(0) as f64,
        }
    }
    pub fn tick(&mut self, now: f64) -> (i32, i32) {
        let distance = (now - self.time).max(0.0) * self.speed;
        let step = |value: f64, target: i32| {
            let target = target as f64;
            if value < target {
                (value + distance).min(target)
            } else {
                (value - distance).max(target)
            }
        };
        self.current = (
            step(self.current.0, self.target.0),
            step(self.current.1, self.target.1),
        );
        self.time = now;
        (to_i32_f64(self.current.0), to_i32_f64(self.current.1))
    }
    pub fn retarget(&mut self, target: (i32, i32), speed: i32) {
        self.target = target;
        self.speed = speed.max(0) as f64;
    }
    pub fn finished(&self) -> bool {
        self.current == (self.target.0 as f64, self.target.1 as f64)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    positions: Vec<i32>,
    soft_wrap: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub lines: Vec<Line>,
    pub line_height: i32,
    len: usize,
}

impl Layout {
    /// `advances` contains one measured advance per character, including newline.
    /// Spaces remain in painted slices. If the first glyph cannot fit, the line is
    /// not split; the complete remaining line stays wide.
    pub fn new(text: &str, advances: &[i32], line_height: i32, width: i32, wrap: bool) -> Self {
        let chars: Vec<_> = text.chars().collect();
        assert_eq!(chars.len(), advances.len());
        let mut layout = Self {
            lines: Vec::new(),
            line_height: line_height.max(1),
            len: chars.len(),
        };
        let mut start = 0;
        loop {
            let hard_end = (start..chars.len())
                .find(|&i| chars[i] == '\n')
                .unwrap_or(chars.len());
            let mut end = hard_end;
            let full = advances[start..hard_end]
                .iter()
                .fold(0i32, |sum, v| sum.saturating_add((*v).max(0)));
            if wrap && full > width.max(1) {
                let mut sum = 0i32;
                for i in start..hard_end {
                    sum = sum.saturating_add(advances[i].max(0));
                    if sum >= width.max(1) {
                        end = if chars[i] == ' ' { i + 1 } else { i };
                        break;
                    }
                }
                if end == start {
                    end = hard_end;
                }
                if end < hard_end && chars[end] != ' ' {
                    if let Some(space) = (start..end).rfind(|&i| chars[i] == ' ') {
                        end = space + 1;
                    }
                }
            }
            let mut positions = vec![0i32];
            for &advance in &advances[start..end] {
                positions.push(positions.last().unwrap().saturating_add(advance.max(0)));
            }
            layout.lines.push(Line {
                start,
                end,
                text: chars[start..end].iter().collect(),
                x: 0,
                y: i32_from(layout.lines.len()) * layout.line_height,
                width: *positions.last().unwrap(),
                positions,
                soft_wrap: end < hard_end,
            });
            if end < hard_end {
                start = end;
            } else if hard_end < chars.len() {
                start = hard_end + 1;
            } else {
                break;
            }
        }
        layout
    }

    fn line_index(&self, index: usize) -> usize {
        let index = index.min(self.len);
        self.lines
            .iter()
            .rposition(|line| line.start <= index)
            .unwrap_or(0)
    }
    pub fn caret(&self, index: usize) -> Rect {
        let line = &self.lines[self.line_index(index)];
        let column = index.saturating_sub(line.start).min(line.end - line.start);
        let x = line.positions[column];
        Rect {
            x: line.x + x,
            y: line.y,
            w: line
                .positions
                .get(column + 1)
                .map_or(1, |right| (right - x).max(1)),
            h: self.line_height,
        }
    }
    fn hit_line(&self, line: usize, x: i32) -> usize {
        let line = &self.lines[line];
        let x = x - line.x;
        for i in 0..line.positions.len() - 1 {
            let right = line.positions[i + 1];
            let advance = right - line.positions[i];
            if x < right - advance / 2 {
                return line.start + i;
            }
        }
        if line.soft_wrap {
            line.end.saturating_sub(1)
        } else {
            line.end
        }
    }
    pub fn hit(&self, x: i32, y: i32) -> usize {
        let line = (y.max(0) / self.line_height) as usize;
        self.hit_line(line.min(self.lines.len() - 1), x)
    }
    /// Selection drag uses the containing glyph, not its midpoint.
    pub fn hit_character(&self, x: i32, y: i32) -> usize {
        let line = &self.lines[((y.max(0) / self.line_height) as usize).min(self.lines.len() - 1)];
        for i in 0..line.positions.len() - 1 {
            if x - line.x < line.positions[i + 1] {
                return line.start + i;
            }
        }
        if line.soft_wrap {
            line.end.saturating_sub(1)
        } else {
            line.end
        }
    }
    pub fn word_at(&self, x: i32, y: i32) -> (usize, usize) {
        let line_index = ((y.max(0) / self.line_height) as usize).min(self.lines.len() - 1);
        let line = &self.lines[line_index];
        let chars: Vec<_> = line.text.chars().collect();
        let index = self.hit_line(line_index, x) - line.start;
        if chars.is_empty() || index >= chars.len() || chars[index] == ' ' {
            return (line.start + index, line.start + index);
        }
        let mut start = index;
        // The search stops at column zero without testing that glyph.
        while start > 0 {
            if chars[start] == ' ' {
                start += 1;
                break;
            }
            start -= 1;
        }
        let mut end = index + 1;
        while end < chars.len() && chars[end] != ' ' {
            end += 1;
        }
        (line.start + start, line.start + end)
    }
    pub fn vertical(&self, index: usize, desired_x: i32, delta_lines: i32) -> usize {
        let line = usize::try_from(
            (self.line_index(index) as i64 + i64::from(delta_lines))
                .clamp(0, self.lines.len() as i64 - 1),
        )
        .unwrap_or(0);
        self.hit_line(line, desired_x)
    }
    pub fn selection(&self, a: usize, b: usize) -> Vec<Rect> {
        let (start, end) = (a.min(b), a.max(b));
        self.lines
            .iter()
            .filter_map(|line| {
                let left = start.max(line.start);
                let right = end.min(line.end);
                (left < right).then(|| Rect {
                    x: line.x + line.positions[left - line.start],
                    y: line.y,
                    w: line.positions[right - line.start] - line.positions[left - line.start],
                    h: self.line_height,
                })
            })
            .collect()
    }
    pub fn clamp_scroll(&self, viewport_w: i32, viewport_h: i32, scroll: (i32, i32)) -> (i32, i32) {
        let width = self
            .lines
            .iter()
            .map(|line| line.width)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let height = i32_from(self.lines.len()) * self.line_height;
        (
            scroll.0.clamp(0, (width - viewport_w.max(1)).max(0)),
            scroll.1.clamp(0, (height - viewport_h.max(1)).max(0)),
        )
    }
    pub fn ensure_visible(
        &self,
        index: usize,
        viewport_w: i32,
        viewport_h: i32,
        scroll: (i32, i32),
    ) -> (i32, i32) {
        let cell = self.caret(index);
        let axis = |position: i32, extent: i32, view: i32, offset: i32| {
            if position < offset {
                position
            } else if position.saturating_add(extent) > offset.saturating_add(view.max(1)) {
                position.saturating_add(extent).saturating_sub(view.max(1))
            } else {
                offset
            }
        };
        self.clamp_scroll(
            viewport_w,
            viewport_h,
            (
                axis(cell.x, cell.w, viewport_w, scroll.0),
                axis(cell.y, cell.h, viewport_h, scroll.1),
            ),
        )
    }
}

/// Word motion for the caret: punctuation remains part of the word.
pub fn word(text: &[char], index: usize, forward: bool) -> usize {
    let blank = |c| c == ' ' || c == '\n';
    let mut i = index.min(text.len());
    if forward {
        while i < text.len() && !blank(text[i]) {
            i += 1;
        }
        while i < text.len() && blank(text[i]) {
            i += 1;
        }
    } else {
        while i > 0 && blank(text[i - 1]) {
            i -= 1;
        }
        while i > 0 && !blank(text[i - 1]) {
            i -= 1;
        }
    }
    i
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn caret_waits_for_idle_then_toggles_once_per_timer_dispatch() {
        let mut blink = CaretBlink::new(0.0);
        blink.tick(1.49);
        assert!(blink.visible);
        blink.tick(1.5);
        assert!(!blink.visible);
        blink.tick(2.49);
        assert!(!blink.visible);
        blink.tick(2.5);
        assert!(blink.visible);
        blink.tick(20.0);
        assert!(!blink.visible);
        blink.activity(20.0);
        assert!(blink.visible);
        blink.tick(21.49);
        assert!(blink.visible);
        blink.tick(21.5);
        assert!(!blink.visible);
    }
    #[test]
    fn smooth_scroll_accumulates_fractional_pixels_and_clamps_target() {
        let mut scroll = SmoothScroll::new((0, 30), (10, 0), 0.0, 150);
        assert_eq!(scroll.tick(0.005), (0, 29));
        assert_eq!(scroll.tick(0.01), (1, 28));
        assert_eq!(scroll.tick(1.0), (10, 0));
        assert!(scroll.finished());
    }
    #[test]
    fn wrapping_preserves_spaces_and_explicit_empty_lines() {
        let s = "aa bb\n\n x";
        let l = Layout::new(s, &vec![4; s.chars().count()], 12, 13, true);
        assert_eq!(
            l.lines.iter().map(|v| v.text.as_str()).collect::<Vec<_>>(),
            ["aa ", "bb", "", " x"]
        );
        assert_eq!(l.lines[3].start, 7);
    }
    #[test]
    fn soft_line_end_is_last_glyph_but_hard_line_has_newline_caret() {
        let layout = Layout::new("abcdef\nx", &[4, 4, 4, 4, 4, 4, 0, 4], 15, 13, true);
        assert_eq!(layout.lines[0].text, "abc");
        assert_eq!(layout.hit(100, 0), 2);
        assert_eq!(layout.hit(100, 15), 6);
        assert_eq!(layout.hit(100, 30), 8);
        let oversized = Layout::new("WWW", &[10, 10, 10], 15, 5, true);
        assert_eq!(oversized.lines.len(), 1);
        assert_eq!(oversized.lines[0].width, 30);
    }
    #[test]
    fn midpoint_ties_advance_and_vertical_motion_uses_pixels() {
        let l = Layout::new("Wi\niWi", &[10, 2, 0, 2, 10, 2], 15, 100, true);
        assert_eq!(l.hit(4, 0), 0);
        assert_eq!(l.hit(5, 0), 1);
        assert_eq!(l.vertical(1, l.caret(1).x, 1), 5);
    }
    #[test]
    fn visibility_includes_whole_glyph_and_selection_splits_lines() {
        let l = Layout::new("ab\ncd", &[8, 8, 0, 8, 8], 15, 100, false);
        assert_eq!(l.ensure_visible(4, 10, 15, (0, 0)), (6, 15));
        assert_eq!(
            l.selection(1, 5),
            [
                Rect {
                    x: 8,
                    y: 0,
                    w: 8,
                    h: 15
                },
                Rect {
                    x: 0,
                    y: 15,
                    w: 16,
                    h: 15
                }
            ]
        );
    }
    #[test]
    fn words_use_only_space_and_newline_boundaries() {
        let chars: Vec<_> = "one,two  three\nfour".chars().collect();
        assert_eq!(word(&chars, 0, true), 9);
        assert_eq!(word(&chars, 9, false), 0);
        assert_eq!(word(&chars, chars.len(), false), 15);
    }
}
