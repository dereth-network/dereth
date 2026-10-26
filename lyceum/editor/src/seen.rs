//! Long lists drawn only where they are seen. Inside a scroll area, the rows (or blocks of rows)
//! outside its view are not drawn: their space is kept, each at the height it took when it was
//! drawn last, so the scroll bar stands as it would with every one drawn.
//!
//! A grid drawn so takes its columns' widths from the rows in view, so its columns keep their
//! width by being given one: a column of names a fixed width, each name cut to it
//! ([`name_cell`]), or a column whose width cannot be known before its rows are drawn the width
//! of the widest row it has shown ([`held_cell`]).

use lyceum_foundry::edit::Weenie;

use super::EditorApp;

/// The longest name a skill usually has: a column of skills' names is as wide as it, and a
/// longer name is cut.
pub const LONGEST_SKILL: &str = "Creature Enchantment";

impl EditorApp {
    /// The rows of weenie `w`'s lists keyed `prefix:<row>:…` (for each of `prefixes`) that have a
    /// field asked for or being typed in: drawn whether they are seen or not, so the field keeps
    /// the keyboard and what is typed in it.
    pub(super) fn held_rows(
        &self,
        w: &Weenie,
        prefixes: &[&str],
    ) -> std::collections::BTreeSet<usize> {
        let typing = super::form::typing_key(w, "");
        let row_of = |key: &str| -> Option<usize> {
            prefixes.iter().find_map(|p| {
                let rest = key.strip_prefix(p)?.strip_prefix(':')?;
                rest.split(':').next()?.parse().ok()
            })
        };
        let focus = self
            .editor
            .focus
            .as_ref()
            .filter(|(fw, _)| fw == w)
            .and_then(|(_, k)| row_of(k));
        self.panels
            .form_typing
            .keys()
            .filter_map(|k| k.strip_prefix(&typing))
            .filter_map(row_of)
            .chain(focus)
            .collect()
    }
}

/// The heights `id` keeps of `n` rows, each `guess` until it is drawn.
fn heights(ui: &egui::Ui, id: egui::Id, n: usize, guess: f32) -> Vec<f32> {
    let mut h: Vec<f32> = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    h.resize(n, guess);
    h
}

/// Whether a row from `top` to `top + height` is outside `clip`.
fn unseen(clip: egui::Rect, top: f32, height: f32) -> bool {
    top + height < clip.min.y || top > clip.max.y
}

/// `n` blocks of a vertical layout, each drawn by `block` where any of it is seen, else its space
/// kept. A block `keep` answers for is drawn seen or not (one a scroll is to bring into view).
/// `guess`: a block's height before it is first drawn.
pub fn blocks(
    ui: &mut egui::Ui,
    id: egui::Id,
    n: usize,
    guess: f32,
    keep: impl Fn(usize) -> bool,
    mut block: impl FnMut(&mut egui::Ui, usize),
) {
    let mut h = heights(ui, id, n, guess);
    let clip = ui.clip_rect();
    let mut skipped = 0.0f32;
    for (i, height) in h.iter_mut().enumerate() {
        let top = ui.cursor().min.y + skipped;
        if !keep(i) && unseen(clip, top, *height) {
            skipped += *height;
            continue;
        }
        if skipped > 0.0 {
            ui.add_space(skipped);
            skipped = 0.0;
        }
        let start = ui.cursor().min.y;
        block(ui, i);
        *height = (ui.cursor().min.y - start).max(0.0);
    }
    if skipped > 0.0 {
        ui.add_space(skipped);
    }
    ui.data_mut(|d| d.insert_temp(id, h));
}

/// `n` rows of a grid, each drawn by `row` (which ends it) where any of it is seen; the rows not
/// seen side by side are one row of space as tall as they are. `keep` and `guess` as for
/// [`blocks`].
pub fn grid_rows(
    ui: &mut egui::Ui,
    id: egui::Id,
    n: usize,
    guess: f32,
    keep: impl Fn(usize) -> bool,
    mut row: impl FnMut(&mut egui::Ui, usize),
) {
    let mut h = heights(ui, id, n, guess);
    let clip = ui.clip_rect();
    let gap = ui.spacing().item_spacing.y;
    let mut skipped = 0.0f32;
    let space = |ui: &mut egui::Ui, height: f32| {
        // One row in place of those not drawn: its height and the gap after it make theirs.
        ui.allocate_space(egui::vec2(1.0, (height - gap).max(0.0)));
        ui.end_row();
    };
    for (i, height) in h.iter_mut().enumerate() {
        let top = ui.cursor().min.y + skipped;
        if !keep(i) && unseen(clip, top, *height) {
            skipped += *height;
            continue;
        }
        if skipped > 0.0 {
            space(ui, skipped);
            skipped = 0.0;
        }
        let start = ui.cursor().min.y;
        row(ui, i);
        *height = (ui.cursor().min.y - start).max(0.0);
    }
    if skipped > 0.0 {
        space(ui, skipped);
    }
    ui.data_mut(|d| d.insert_temp(id, h));
}

/// `n` rows of one height in `area`, a vertical scroll, each drawn by `row` where it is seen.
/// Each row is taken to be as tall as the tallest of those drawn last (a button's height before
/// any is), so the rows drawn fill the view down to its bottom, whatever the font and the rows'
/// widgets make of a row's height.
pub fn rows(
    ui: &mut egui::Ui,
    id: egui::Id,
    area: egui::ScrollArea,
    n: usize,
    mut row: impl FnMut(&mut egui::Ui, usize),
) {
    let gap = ui.spacing().item_spacing.y;
    let kept: Option<f32> = ui.data(|d| d.get_temp(id));
    let height = kept.unwrap_or(ui.spacing().interact_size.y);
    let mut tallest = 0.0f32;
    area.show_rows(ui, height, n, |ui, shown| {
        for i in shown {
            let top = ui.cursor().min.y;
            row(ui, i);
            tallest = tallest.max(ui.cursor().min.y - top - gap);
        }
    });
    if tallest > 0.0 && kept != Some(tallest) {
        ui.data_mut(|d| d.insert_temp(id, tallest));
    }
}

/// The width the widest of `texts` takes in the panels' body font, in points, a point to spare.
#[must_use]
pub fn widest<'a>(ui: &egui::Ui, texts: impl IntoIterator<Item = &'a str>) -> f32 {
    texts
        .into_iter()
        .map(|s| {
            egui::WidgetText::from(s)
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Body,
                )
                .size()
                .x
        })
        .fold(0.0, f32::max)
        .ceil()
        + 1.0
}

/// A grid's cell `width` wide holding `text` on one line, cut with "…" where it is longer: a
/// column of names that keeps its width whatever names are in view. A name cut is not shown
/// whole on hover.
pub fn name_cell(
    ui: &mut egui::Ui,
    text: impl Into<egui::WidgetText>,
    width: f32,
) -> egui::Response {
    ui.scope(|ui| {
        ui.set_width(width);
        ui.add(
            egui::Label::new(text)
                .truncate()
                .show_tooltip_when_elided(false),
        )
    })
    .inner
}

/// A grid's cell drawn by `add`, at least as wide as the widest of `id`'s cells has been: a
/// column of a list drawn only where it is seen widens as a wider row comes into view, and never
/// narrows again as rows go out of it.
pub fn held_cell<R>(ui: &mut egui::Ui, id: egui::Id, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let widest: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    let r = ui.scope(|ui| {
        ui.set_min_width(widest);
        add(ui)
    });
    let width = r.response.rect.width();
    if width > widest {
        ui.data_mut(|d| d.insert_temp(id, width));
    }
    r.inner
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's long lists)
    use super::*;

    /// Three frames of a 400 by 300 window scrolled to `offset`, a list of 1000 rows in its scroll
    /// area as `list` draws them: the rows drawn in the last frame, and how tall the list stood.
    fn frames(offset: f32, list: impl Fn(&mut egui::Ui, &mut Vec<usize>)) -> (Vec<usize>, f32) {
        let ctx = egui::Context::default();
        let mut drawn = Vec::new();
        let mut tall = 0.0;
        for _ in 0..3 {
            drawn.clear();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                egui::ScrollArea::vertical()
                    .vertical_scroll_offset(offset)
                    .show(ui, |ui| {
                        list(ui, &mut drawn);
                        tall = ui.min_rect().height();
                    });
            });
            // Nothing is drawn: the fonts' textures it made are let go.
            output.textures_delta.clear();
        }
        (drawn, tall)
    }

    /// A row's contents: 100 by 20 points.
    fn row(ui: &mut egui::Ui) {
        let _ = ui.allocate_exact_size(egui::vec2(100.0, 20.0), egui::Sense::hover());
    }

    #[test]
    fn a_long_list_draws_only_the_blocks_its_view_sees_and_keeps_the_others_space() {
        let list = |ui: &mut egui::Ui, drawn: &mut Vec<usize>| {
            blocks(
                ui,
                egui::Id::new("list"),
                1000,
                30.0,
                |i| i == 900,
                |ui, i| {
                    drawn.push(i);
                    row(ui);
                },
            );
        };
        let (top, tall) = frames(0.0, list);
        assert!(top.len() < 40 && top.contains(&0), "{top:?}");
        assert!(top.contains(&900), "the one kept");
        // Each row's space kept: its own once drawn, else the guess.
        assert!(tall >= 1000.0 * 23.0 - 1.0, "{tall}");
        let (lower, _) = frames(5000.0, list);
        assert!(!lower.contains(&0) && lower.len() < 40, "{lower:?}");
    }

    #[test]
    fn a_long_grid_draws_only_the_rows_its_view_sees() {
        let list = |ui: &mut egui::Ui, drawn: &mut Vec<usize>| {
            egui::Grid::new("grid").num_columns(2).show(ui, |ui| {
                row(ui);
                row(ui);
                ui.end_row();
                grid_rows(
                    ui,
                    egui::Id::new("grid rows"),
                    1000,
                    30.0,
                    |_| false,
                    |ui, i| {
                        drawn.push(i);
                        row(ui);
                        row(ui);
                        ui.end_row();
                    },
                );
            });
        };
        let (top, tall) = frames(0.0, list);
        assert!(top.len() < 40 && top.contains(&0), "{top:?}");
        assert!(tall >= 1001.0 * 23.0 - 1.0, "{tall}");
        let (lower, _) = frames(5000.0, list);
        assert!(!lower.contains(&0) && lower.len() < 40, "{lower:?}");
    }

    /// A context with the panels' own fonts and look, as the editor's panels are drawn with.
    fn panels() -> egui::Context {
        crate::gui::Gui::new().ctx
    }

    /// One frame of a window `size` points, drawn by `draw`: the shapes it painted.
    fn frame(
        ctx: &egui::Context,
        size: egui::Vec2,
        draw: impl FnMut(&mut egui::Ui),
    ) -> Vec<egui::epaint::ClippedShape> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, draw);
        output.textures_delta.clear();
        output.shapes
    }

    /// A row as the lists' are: a number, a small button, a name to choose and a note.
    fn list_row(ui: &mut egui::Ui, i: usize) -> egui::Rect {
        ui.horizontal(|ui| {
            ui.monospace(format!("{i:04X}"));
            let _ = ui.small_button("×");
            let _ = ui.selectable_label(false, format!("Dungeon {i}"));
            ui.weak("12 cells");
        })
        .response
        .rect
    }

    #[test]
    fn a_list_of_rows_fills_a_tall_view_to_its_bottom() {
        // A view as tall as the world view's, where a row height guessed two points too tall
        // stopped the rows three rows short of the bottom.
        for offset in [0.0, 2000.0, 2345.5] {
            let ctx = panels();
            let (mut bottom, mut view) = (0.0f32, egui::Rect::NOTHING);
            for _ in 0..3 {
                bottom = 0.0;
                frame(&ctx, egui::vec2(400.0, 1000.0), |ui| {
                    let area = egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .vertical_scroll_offset(offset);
                    rows(ui, egui::Id::new("list"), area, 1000, |ui, i| {
                        view = ui.clip_rect();
                        bottom = bottom.max(list_row(ui, i).bottom());
                    });
                });
            }
            assert!(view.height() > 900.0, "{view:?}");
            assert!(
                bottom >= view.bottom() - 1.0,
                "{offset}: {bottom} in {view:?}"
            );
        }
    }

    /// The left edge of each drawn row's second cell in a grid of 1000 rows scrolled to `offset`
    /// in `ctx`, its first cell drawn by `first` with row `i`.
    fn second_column(
        ctx: &egui::Context,
        offset: f32,
        first: &impl Fn(&mut egui::Ui, usize),
    ) -> Vec<f32> {
        let mut lefts = Vec::new();
        for _ in 0..3 {
            lefts.clear();
            frame(ctx, egui::vec2(600.0, 300.0), |ui| {
                egui::ScrollArea::vertical()
                    .vertical_scroll_offset(offset)
                    .show(ui, |ui| {
                        egui::Grid::new("grid").num_columns(2).show(ui, |ui| {
                            grid_rows(
                                ui,
                                egui::Id::new("grid rows"),
                                1000,
                                22.0,
                                |_| false,
                                |ui, i| {
                                    first(ui, i);
                                    lefts.push(ui.label(format!("{i}")).rect.left());
                                    ui.end_row();
                                },
                            );
                        });
                    });
            });
        }
        lefts
    }

    /// A skill's name: short at the top of the list, the longest further down.
    fn skill_name(i: usize) -> &'static str {
        if i < 100 {
            "Run"
        } else {
            "Creature Enchantment"
        }
    }

    #[test]
    fn a_column_of_names_keeps_its_width_as_rows_scroll_in_and_out_of_view() {
        let ctx = panels();
        let names = |ui: &mut egui::Ui, i: usize| {
            let width = widest(ui, ["Creature Enchantment"]);
            name_cell(ui, skill_name(i), width);
        };
        let top = second_column(&ctx, 0.0, &names);
        let lower = second_column(&ctx, 5000.0, &names);
        assert!(!top.is_empty() && !lower.is_empty());
        assert!(
            top.iter().chain(&lower).all(|x| (x - top[0]).abs() < 0.5),
            "{top:?} {lower:?}"
        );
        // As a plain label, the column is as wide as the names in view.
        let plain = panels();
        let label = |ui: &mut egui::Ui, i: usize| {
            ui.label(skill_name(i));
        };
        let (top, lower) = (
            second_column(&plain, 0.0, &label),
            second_column(&plain, 5000.0, &label),
        );
        assert!(lower[0] > top[0] + 20.0, "{top:?} {lower:?}");
    }

    #[test]
    fn a_name_longer_than_its_column_is_cut_and_one_that_fits_is_not() {
        let ctx = panels();
        let mut cells = Vec::new();
        let shapes = frame(&ctx, egui::vec2(600.0, 300.0), |ui| {
            let width = widest(ui, ["Creature Enchantment"]);
            egui::Grid::new("names").show(ui, |ui| {
                for name in ["Creature Enchantment", "Enhancement of the Blade Turner"] {
                    cells.push((width, name_cell(ui, name, width).rect));
                    ui.end_row();
                }
            });
        });
        for (width, rect) in &cells {
            assert!(rect.width() <= width + 0.5, "{rect:?} in {width}");
        }
        let elided = |text: &str| {
            shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == text => Some(t.galley.elided),
                _ => None,
            })
        };
        assert_eq!(elided("Creature Enchantment"), Some(false));
        assert_eq!(elided("Enhancement of the Blade Turner"), Some(true));
    }

    #[test]
    fn a_held_column_widens_for_a_wider_row_in_view_and_does_not_narrow_after() {
        let ctx = panels();
        let wide = |i: usize| (200..300).contains(&i);
        let first = |ui: &mut egui::Ui, i: usize| {
            held_cell(ui, egui::Id::new("first column"), |ui| {
                let w = if wide(i) { 160.0 } else { 40.0 };
                let _ = ui.allocate_exact_size(egui::vec2(w, 18.0), egui::Sense::hover());
            });
        };
        let narrow = second_column(&ctx, 0.0, &first)[0];
        let widened = second_column(&ctx, 250.0 * 22.0, &first)[0];
        let back = second_column(&ctx, 0.0, &first)[0];
        assert!(widened > narrow + 100.0, "{narrow} {widened}");
        assert!((back - widened).abs() < 0.5, "{widened} {back}");
    }
}
