//! Where a window opens the first time: the place its window asks for, moved as little as it
//! takes to keep clear of the HUD's elements as the default layout puts them (the target bar, the
//! status row and the conflict lamp, the minimap and its place line, the pets, the party list's
//! rows, the log window, the hotbar with its attack and Dismount buttons, the parameter bar and the
//! menu bar), and then of the windows already open where it can. A window too large to clear the
//! HUD stands where it covers the least of it.

use dereth_ui_kit::paint::Painter;

use crate::draw::Rect;
use crate::state::GameState;
use crate::ui::{chat, hotbar, hud, minimap, party, pets};

/// How far a window keeps from the HUD's elements and the screen's edges, in layout units.
pub const GAP: f32 = 8.0;

/// What the HUD's elements cover at the default layout on a screen of `screen` pixels, at `k`
/// pixels a layout unit, for the game as `state` has it: the party list as many rows as it shows,
/// the pets' panel only with pets, the rest whether drawn yet or not.
#[must_use]
pub fn hud(screen: (f32, f32), k: f32, state: &GameState) -> Vec<Rect> {
    let span = |a: Rect, b: Rect| {
        let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
        Rect::new(
            x0,
            y0,
            a.right().max(b.right()) - x0,
            a.bottom().max(b.bottom()) - y0,
        )
    };
    let target = hud::target_rect(screen, k);
    let effects = hud::effects_rect(screen, k);
    let m = minimap::parts(screen, k);
    let gauges = hud::gauge_rects(screen, k);
    let gauges = span(gauges[0], gauges[2]);
    let gauges = Rect::new(gauges.x, gauges.y - 18.0 * k, gauges.w, gauges.h + 18.0 * k);
    let rows = state.bar.as_ref().map_or(1, |b| b.rows.max(1));
    let (_, _, number) = hotbar::set_rects(screen, k);
    let bar = span(
        hotbar::slot_rect(screen, k, rows - 1, 0),
        span(number, hotbar::attack_rect(screen, k)),
    );
    let mut out = vec![
        target,
        effects,
        party::lamp_rect(screen, k),
        span(m.map, m.place),
        chat::Chat::rect(screen, k),
        span(bar, pets::dismount_rect(screen, k)),
        gauges,
        hud::menu_rect(screen, k),
    ];
    let fellows = party::rows(state).len();
    if fellows > 0 {
        out.push(span(party::row_rect(k, 0), party::row_rect(k, fellows - 1)));
    }
    out.extend(pets::PetsPanel::layout(state, screen, k).map(|l| l.panel));
    out
}

std::thread_local! {
    /// The windows open as the interface draws the next one on this thread, which a window
    /// opening keeps clear of where it can once it is clear of the HUD.
    static OPEN: std::cell::RefCell<Vec<Rect>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The windows open as the next window is drawn are `rects`.
pub fn set_open(rects: Vec<Rect>) {
    OPEN.with(|o| *o.borrow_mut() = rects);
}

/// One more window is open: `r`.
pub fn add_open(r: Rect) {
    OPEN.with(|o| o.borrow_mut().push(r));
}

/// Where a window `size` layout units large opens, asking for `wanted` (its top-left corner, in
/// pixels), on the painter's screen for the game as `state` has it: `wanted` when it is clear of
/// the HUD and the windows open, else the nearest place on the screen clear of both, else the
/// nearest clear of the HUD that covers the least of the windows, else the place that covers the
/// least of the HUD.
#[must_use]
pub fn clear(
    p: &Painter<'_>,
    state: &GameState,
    size: (f32, f32),
    wanted: (f32, f32),
) -> (f32, f32) {
    let k = p.scale;
    let open = OPEN.with(|o| o.borrow().clone());
    clear_of(
        p.screen,
        (size.0 * k, size.1 * k),
        wanted,
        &hud(p.screen, k, state),
        &open,
        GAP * k,
    )
}

/// Where a window `size` pixels large stands, asking for `wanted`, on a screen of `screen`
/// pixels, kept `gap` from each of `hud` and `open` and from the screen's edges: see [`clear`].
#[must_use]
pub fn clear_of(
    screen: (f32, f32),
    size: (f32, f32),
    wanted: (f32, f32),
    hud: &[Rect],
    open: &[Rect],
    gap: f32,
) -> (f32, f32) {
    let (w, h) = size;
    let grown = |rs: &[Rect]| -> Vec<Rect> {
        rs.iter()
            .map(|r| Rect::new(r.x - gap, r.y - gap, r.w + 2.0 * gap, r.h + 2.0 * gap))
            .collect()
    };
    let (kept, apart) = (grown(hud), grown(open));
    let covered = |x: f32, y: f32, of: &[Rect]| -> f32 {
        let r = Rect::new(x, y, w, h);
        of.iter()
            .filter_map(|o| r.intersect(o))
            .map(|i| i.w * i.h)
            .sum()
    };
    let wanted = (
        wanted.0.clamp(0.0, (screen.0 - w).max(0.0)),
        wanted.1.clamp(0.0, (screen.1 - h).max(0.0)),
    );
    if covered(wanted.0, wanted.1, &kept) <= 0.0 && covered(wanted.0, wanted.1, &apart) <= 0.0 {
        return wanted;
    }
    let mut xs = vec![wanted.0, gap, screen.0 - w - gap];
    let mut ys = vec![wanted.1, gap, screen.1 - h - gap];
    for r in hud.iter().chain(open) {
        xs.extend([r.right() + gap, r.x - gap - w]);
        ys.extend([r.bottom() + gap, r.y - gap - h]);
    }
    let fits = |v: f32, len: f32, side: f32| v >= 0.0 && v + len <= side;
    // Least of the HUD covered, then least of the windows, then nearest; a pixel's difference
    // is none.
    let mut best: Option<([f32; 3], (f32, f32))> = None;
    for &x in xs.iter().filter(|x| fits(**x, w, screen.0)) {
        for &y in ys.iter().filter(|y| fits(**y, h, screen.1)) {
            let score = [
                covered(x, y, &kept),
                covered(x, y, &apart),
                (x - wanted.0).abs() + (y - wanted.1).abs(),
            ];
            let better = best.is_none_or(|(b, _)| {
                for (s, b) in score.iter().zip(b) {
                    if (s - b).abs() > 1.0 {
                        return s < &b;
                    }
                }
                false
            });
            if better {
                best = Some((score, (x, y)));
            }
        }
    }
    let (x, y) = best.map_or(wanted, |(_, at)| at);
    (x.round(), y.round())
}
