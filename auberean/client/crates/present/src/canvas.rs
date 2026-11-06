//! What changed in an interface picture since the one the device holds: the device takes only that.

/// A rectangle of texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Rows this close together go up as one rectangle: a copy has a cost of its own.
const JOIN_ROWS: usize = 8;

/// The rectangles of `new` that differ from `old`, top to bottom, each the box of its changed
/// rows. Both are `width` by `height`, four bytes a texel.
#[must_use]
pub fn changed_spans(old: &[u8], new: &[u8], width: u32, height: u32) -> Vec<Span> {
    let stride = width as usize * 4;
    if stride == 0 || old.len() != new.len() {
        return Vec::new();
    }
    let mut spans: Vec<Span> = Vec::new();
    let mut last_row = 0usize;
    for (y, (a, b)) in old
        .chunks_exact(stride)
        .zip(new.chunks_exact(stride))
        .enumerate()
    {
        if a == b || y >= height as usize {
            continue;
        }
        let differs = |(p, q): (&[u8; 4], &[u8; 4])| p != q;
        let texels = || a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0);
        let first = texels().position(differs).unwrap_or(0);
        let last = texels().rposition(differs).unwrap_or(first);
        #[allow(clippy::cast_possible_truncation)] // within the picture's size
        let (x0, x1, row) = (first as u32, last as u32, y as u32);
        match spans.last_mut() {
            Some(s) if y - last_row <= JOIN_ROWS => {
                let right = (s.x + s.width - 1).max(x1);
                s.x = s.x.min(x0);
                s.width = right - s.x + 1;
                s.height = row - s.y + 1;
            }
            _ => spans.push(Span {
                x: x0,
                y: row,
                width: x1 - x0 + 1,
                height: 1,
            }),
        }
        last_row = y;
    }
    spans
}

/// `spans` of `new` copied over `old`.
pub fn copy_spans(old: &mut [u8], new: &[u8], width: u32, spans: &[Span]) {
    let stride = width as usize * 4;
    for s in spans {
        for y in s.y as usize..(s.y + s.height) as usize {
            let from = y * stride + s.x as usize * 4;
            let to = from + s.width as usize * 4;
            old[from..to].copy_from_slice(&new[from..to]);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the same picture by a quicker way, which no behavior of the game names)

    use super::*;

    fn picture(width: u32, height: u32) -> Vec<u8> {
        vec![7; (width * height * 4) as usize]
    }

    fn poke(p: &mut [u8], width: u32, x: u32, y: u32) {
        p[((y * width + x) * 4 + 1) as usize] ^= 0xFF;
    }

    /// No change is nothing to send; a single texel is one texel, even at the picture's corners.
    #[test]
    fn a_change_is_the_box_of_what_changed() {
        let (w, h) = (16, 12);
        let old = picture(w, h);
        assert!(changed_spans(&old, &old, w, h).is_empty());
        for (x, y) in [(0, 0), (15, 11), (3, 5)] {
            let mut new = old.clone();
            poke(&mut new, w, x, y);
            let one = Span {
                x,
                y,
                width: 1,
                height: 1,
            };
            assert_eq!(changed_spans(&old, &new, w, h), [one]);
        }
    }

    /// Rows close together go as one box; rows far apart as two.
    #[test]
    fn rows_join_when_they_are_close() {
        let (w, h) = (20, 40);
        let old = picture(w, h);
        let mut near = old.clone();
        poke(&mut near, w, 2, 1);
        poke(&mut near, w, 9, 5);
        assert_eq!(
            changed_spans(&old, &near, w, h),
            [Span {
                x: 2,
                y: 1,
                width: 8,
                height: 5
            }]
        );
        let mut far = old.clone();
        poke(&mut far, w, 2, 1);
        poke(&mut far, w, 9, 30);
        assert_eq!(changed_spans(&old, &far, w, h).len(), 2);
    }

    /// What goes up leaves the kept copy the same as the new picture, whatever changed.
    #[test]
    fn the_spans_copied_make_the_new_picture() {
        let (w, h) = (31, 23);
        let old = picture(w, h);
        let mut seed = 12345u32;
        for _ in 0..200 {
            let mut new = old.clone();
            for _ in 0..(seed % 9) {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                poke(&mut new, w, (seed >> 8) % w, (seed >> 16) % h);
            }
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let mut kept = old.clone();
            copy_spans(&mut kept, &new, w, &changed_spans(&old, &new, w, h));
            assert_eq!(kept, new);
        }
    }
}
