//! The chat log's rows and its text selection. The log can be selected like the era's other
//! selectable text: a press and drag selects, a double click selects a word, and the copy key
//! copies what is selected. Rows are wrapped the way the log draws them, and every row keeps its
//! place in its line's text, so a copy gives back the text as it was received.

/// A place in the log: a line of chat and a byte offset in its text.
pub type Place = (usize, usize);

/// One drawn row: the line it belongs to and the part of that line's text it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    /// The row's top, from the top of the whole log.
    pub y: i32,
}

/// Where each word of a paragraph starts, and the word.
fn words(paragraph: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut rest = paragraph;
    let mut at = 0;
    std::iter::from_fn(move || {
        let skip = rest.len() - rest.trim_start().len();
        rest = &rest[skip..];
        at += skip;
        if rest.is_empty() {
            return None;
        }
        let len = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = (at, &rest[..len]);
        rest = &rest[len..];
        at += len;
        Some(word)
    })
}

/// The rows a text wraps into at `width`, as byte ranges of the text: whole words while they
/// fit, a word wider than the row broken between letters.
pub fn wrap(text: &str, width: i32, measure: &dyn Fn(&str) -> i32) -> Vec<(usize, usize)> {
    let mut rows = vec![];
    let mut base = 0;
    for paragraph in text.split('\n') {
        let mut line: Option<(usize, usize)> = None;
        for (offset, word) in words(paragraph) {
            let (start, end) = (base + offset, base + offset + word.len());
            if let Some((from, _)) = line.filter(|(a, b)| a < b) {
                if measure(&text[from..end]) > width {
                    rows.extend(line.take());
                }
            }
            if let Some((_, to)) = line.as_mut() {
                // The space between words belongs to the row it follows.
                *to = start;
            }
            for (i, ch) in word.char_indices() {
                let next = start + i + ch.len_utf8();
                match line {
                    Some((from, to)) if from < to && measure(&text[from..next]) > width => {
                        rows.push((from, to));
                        line = Some((start + i, next));
                    }
                    Some((from, _)) => line = Some((from, next)),
                    None => line = Some((start + i, next)),
                }
            }
        }
        rows.push(line.unwrap_or((base, base)));
        base += paragraph.len() + 1;
    }
    rows
}

/// Every row of the log, top to bottom. A line takes its rows' height, and never less than
/// `min_line`.
pub fn rows(
    lines: &[&str],
    width: i32,
    row_height: i32,
    min_line: i32,
    measure: &dyn Fn(&str) -> i32,
) -> (Vec<Row>, i32) {
    let mut out = vec![];
    let mut y = 0;
    for (line, text) in lines.iter().enumerate() {
        let wrapped = wrap(text, width, measure);
        let count = i32::try_from(wrapped.len()).unwrap_or(i32::MAX);
        for (i, (start, end)) in wrapped.into_iter().enumerate() {
            let i = i32::try_from(i).unwrap_or(i32::MAX);
            out.push(Row {
                line,
                start,
                end,
                y: y + i * row_height,
            });
        }
        y += (count * row_height).max(min_line);
    }
    (out, y)
}

/// The place nearest a point, in the log's own coordinates (x from the rows' left edge, y from
/// the top of the whole log).
pub fn place_at(
    lines: &[&str],
    rows: &[Row],
    row_height: i32,
    x: i32,
    y: i32,
    measure: &dyn Fn(&str) -> i32,
) -> Option<Place> {
    let row = rows
        .iter()
        .rev()
        .find(|r| r.y <= y)
        .or_else(|| rows.first())?;
    if y >= row.y + row_height && rows.last() == Some(row) {
        return Some((row.line, row.end));
    }
    let text = lines[row.line];
    let offset = text[row.start..row.end]
        .char_indices()
        .map(|(i, _)| row.start + i)
        .chain([row.end])
        .min_by_key(|&at| (measure(&text[row.start..at]) - x).abs())
        .unwrap_or(row.start);
    Some((row.line, offset))
}

/// The selection in order: its first place, then its last.
pub fn ordered(selection: (Place, Place)) -> (Place, Place) {
    let (a, b) = selection;
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// The part of each row the selection covers, as an x range from the row's left edge.
pub fn highlights(
    lines: &[&str],
    rows: &[Row],
    selection: (Place, Place),
    measure: &dyn Fn(&str) -> i32,
) -> Vec<(Row, i32, i32)> {
    let (first, last) = ordered(selection);
    rows.iter()
        .filter_map(|row| {
            let from = if row.line == first.0 {
                first.1.max(row.start)
            } else {
                row.start
            };
            let to = if row.line == last.0 {
                last.1.min(row.end)
            } else {
                row.end
            };
            let inside = (first.0..=last.0).contains(&row.line) && from < to;
            inside.then(|| {
                let text = lines[row.line];
                (
                    *row,
                    measure(&text[row.start..from]),
                    measure(&text[row.start..to]),
                )
            })
        })
        .collect()
}

/// The selected text, one line of chat to a line.
pub fn text(lines: &[&str], selection: (Place, Place)) -> String {
    let (first, last) = ordered(selection);
    (first.0..=last.0.min(lines.len().saturating_sub(1)))
        .map(|line| {
            let text = lines[line];
            let from = if line == first.0 {
                first.1.min(text.len())
            } else {
                0
            };
            let to = if line == last.0 {
                last.1.min(text.len())
            } else {
                text.len()
            };
            &text[from..to.max(from)]
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The word around a place: the run of non-space characters it touches.
pub fn word(lines: &[&str], (line, at): Place) -> (Place, Place) {
    let text = lines[line];
    let at = at.min(text.len());
    let start = text[..at]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let end = text[at..]
        .find(char::is_whitespace)
        .map_or(text.len(), |i| at + i);
    ((line, start), (line, end))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;

    /// Every character is 10 pixels wide.
    fn measure(s: &str) -> i32 {
        i32::try_from(s.chars().count()).unwrap() * 10
    }

    #[test]
    fn rows_wrap_at_word_edges_and_keep_their_place_in_the_text() {
        let text = "You say, \"Hello there\"";
        let rows = wrap(text, 100, &measure);
        let shown: Vec<&str> = rows.iter().map(|&(a, b)| text[a..b].trim_end()).collect();
        assert_eq!(shown, ["You say,", "\"Hello", "there\""]);
        // A word wider than the row breaks between letters.
        let rows = wrap("abcdefghijklmn", 50, &measure);
        assert_eq!(rows, [(0, 5), (5, 10), (10, 14)]);
        assert_eq!(wrap("", 50, &measure), [(0, 0)]);
        assert_eq!(wrap("a\nb", 50, &measure), [(0, 1), (2, 3)]);
    }

    #[test]
    fn a_drag_across_lines_selects_from_the_press_to_the_release() {
        let lines = ["first line", "second line here"];
        let (rows, total) = rows(&lines, 100, 15, 15, &measure);
        assert_eq!(total, 45);
        // Press after "first", drag to after "second".
        let a = place_at(&lines, &rows, 15, 50, 5, &measure).unwrap();
        let b = place_at(&lines, &rows, 15, 60, 20, &measure).unwrap();
        assert_eq!((a, b), ((0, 5), (1, 6)));
        assert_eq!(text(&lines, (b, a)), " line\nsecond");
        let lit = highlights(&lines, &rows, (a, b), &measure);
        assert_eq!(
            lit.iter()
                .map(|(r, x0, x1)| (r.y, *x0, *x1))
                .collect::<Vec<_>>(),
            [(0, 50, 100), (15, 0, 60)]
        );
        // Below the last row is the end of the log.
        assert_eq!(place_at(&lines, &rows, 15, 0, 200, &measure), Some((1, 16)));
    }

    #[test]
    fn a_double_click_selects_the_word_under_the_pointer() {
        let lines = ["Pathwarden Thorolf tells you, \"Welcome\""];
        assert_eq!(word(&lines, (0, 13)), ((0, 11), (0, 18)));
        assert_eq!(text(&lines, word(&lines, (0, 13))), "Thorolf");
    }
}
