//! The plugin host's own tier, above every plugin window: its windows (the plugin panel) and its
//! question to the player (a file a plugin asks for). Both are drawn by the kit in the skin of the
//! interface they stand in, as any window is. The question is a dialog with its own buttons:
//! Escape refuses it, and its answer carries the question's id, so it answers that question alone.

use dereth_client_contract::host_question::{HostQuestion, HostReply};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::draw::Rect;
use dereth_ui_kit::input::vk;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::window::PluginUi;
use crate::{Tier, WindowView};

/// A question's width and the most rows of a list it shows at once, in layout units.
const WIDTH: f32 = 440.0;
const ROWS: usize = 8;

/// Where a question's parts were drawn last frame, in screen pixels.
#[derive(Debug, Default, Clone)]
pub struct AskDrawn {
    /// The whole dialog.
    pub frame: Rect,
    /// Each line to pick that is in view, by its place in the question's list.
    pub picks: Vec<(usize, Rect)>,
    /// The buttons, first to last.
    pub buttons: Vec<Rect>,
}

/// The question the player is answering: what they have done on it so far.
#[derive(Debug, Default)]
pub struct Asking {
    id: Option<u64>,
    picked: Option<usize>,
    scroll: f32,
    /// How far its words are scrolled, where they are taller than the screen leaves.
    words: f32,
    /// The dialog's place, which the player may drag it from.
    window: WindowState,
    /// Where it was drawn last.
    pub drawn: AskDrawn,
}

impl Asking {
    /// Draw question `q` as a dialog in the interface's own look (centred when it is first put),
    /// take the player's input on it, and answer the player's reply once one of its buttons is
    /// pressed, or Escape refuses it.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        q: &HostQuestion,
    ) -> Option<HostReply> {
        if self.id != Some(q.id) {
            *self = Self {
                id: Some(q.id),
                window: WindowState {
                    open: true,
                    opened_at: ctx.time,
                    no_close: true,
                    ..WindowState::default()
                },
                ..Self::default()
            };
        }
        let k = p.scale;
        let (sw, sh) = p.screen;
        let body = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 13.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let width = WIDTH.min(sw / k - 16.0).max(120.0);
        let inner_w = width * k;
        let lh = p.line_height(&body).max(16.0 * k);
        let wrap = |p: &Painter<'_>, style: &TextStyle, text: &str| -> Vec<String> {
            text.split('\n')
                .flat_map(|l| {
                    if l.trim().is_empty() {
                        vec![String::new()]
                    } else {
                        p.wrap(style, l, inner_w)
                    }
                })
                .collect()
        };
        let asker = if q.asker.is_empty() {
            Vec::new()
        } else {
            wrap(p, &dim, &format!("Asked for {}", q.asker))
        };
        let text = wrap(p, &body, &q.text);
        let row_h = (lh + 4.0 * k).round();
        #[allow(clippy::cast_precision_loss)]
        let list_h = if q.picks.is_empty() {
            0.0
        } else {
            row_h * q.picks.len().min(ROWS) as f32 + 4.0 * k
        };
        #[allow(clippy::cast_precision_loss)]
        let words_h = lh * (asker.len() + text.len()) as f32;
        let button_h = 28.0 * k;
        // The dialog keeps to the screen, its buttons always on it: words taller than the room
        // left scroll.
        let fixed = if list_h > 0.0 { list_h + 8.0 * k } else { 0.0 } + 10.0 * k + button_h;
        let inset = kit::window_body_inset(p);
        let room = (sh - 16.0 * k - (inset[1] + inset[3]) * k - fixed).max(2.0 * lh);
        let shown_h = words_h.min(room);
        let size = kit::window_size_for(p, (width, (shown_h + fixed) / k));
        let centred = (
            ((sw - size.0 * k) / 2.0).max(0.0).round(),
            ((sh - size.1 * k) / 2.0).max(8.0 * k).round(),
        );
        let out = kit::window(p, ctx, &mut self.window, &q.title, size, centred);
        ctx.input.nav.mark_box();
        let inner = out.body;
        let mut drawn = AskDrawn {
            frame: out.rect,
            ..AskDrawn::default()
        };
        let words = Rect::new(inner.x, inner.y, inner.w, shown_h);
        let offset = if words_h > shown_h + 0.5 {
            kit::scroll(p, ctx, words, words_h, &mut self.words)
        } else {
            0.0
        };
        p.list.push_clip(words);
        let mut y = inner.y - offset;
        for l in &asker {
            p.text(&dim, inner.x, y, l);
            y += lh;
        }
        for l in &text {
            p.text(&body, inner.x, y, l);
            y += lh;
        }
        p.list.pop_clip();
        if list_h > 0.0 {
            let area = Rect::new(inner.x, words.bottom() + 4.0 * k, inner.w, list_h);
            let pal = *p.art.palette();
            p.fill(area, pal.list_ground);
            #[allow(clippy::cast_precision_loss)]
            let content = row_h * q.picks.len() as f32 + 4.0 * k;
            let offset = kit::scroll(p, ctx, area, content, &mut self.scroll);
            p.list.push_clip(area);
            for (i, line) in q.picks.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let ry = area.y + 2.0 * k + row_h * i as f32 - offset;
                let row = Rect::new(area.x + 2.0 * k, ry, area.w - 12.0 * k, row_h);
                if row.bottom() < area.y || row.y > area.bottom() {
                    continue;
                }
                let over = ctx.over(&row);
                kit::list_highlight(p, row, self.picked == Some(i));
                if over && self.picked != Some(i) {
                    p.fill(row, pal.row_hover);
                }
                p.text_in(&body, row.inset(4.0 * k), Align::Left, line);
                if over && ctx.input.clicked(&row) {
                    self.picked = Some(i);
                }
                drawn.picks.push((i, row));
            }
            p.list.pop_clip();
        }
        // The buttons, at the bottom right: the first gives the answer yes, the last no.
        let labels: Vec<&str> = std::iter::once(q.yes.as_str())
            .chain(q.no.as_deref())
            .collect();
        let bw = 120.0 * k;
        let by = inner.bottom() - button_h;
        #[allow(clippy::cast_precision_loss)]
        let mut bx = inner.right() - (bw + 8.0 * k) * labels.len() as f32 + 8.0 * k;
        let mut pressed = None;
        for (i, label) in labels.iter().enumerate() {
            let b = Rect::new(bx, by, bw, button_h);
            let enabled = i > 0 || !q.needs_pick || self.picked.is_some();
            if kit::button(p, ctx, b, label, enabled) && enabled {
                pressed = Some(i);
            }
            if i + 1 == labels.len() {
                ctx.input.nav.cancel(b);
            }
            drawn.buttons.push(b);
            bx += bw + 8.0 * k;
        }
        ctx.input.nav.end_layer();
        let mut answer = pressed.map(|i| i == 0);
        if ctx.input.take_key(vk::ESCAPE) {
            answer = Some(false);
        }
        // A press on the dialog is its own.
        if ctx.input.hover(&out.rect) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
            ctx.hot = true;
        }
        self.drawn = drawn;
        Some(HostReply {
            id: q.id,
            yes: answer?,
            picked: self.picked,
        })
    }

    /// The id of the question drawn last, while one is up.
    #[must_use]
    pub fn id(&self) -> Option<u64> {
        self.id
    }

    /// The question is down: what was done on it goes with it.
    pub fn forget(&mut self) {
        *self = Self::default();
    }
}

/// The host's own tier: its windows (the plugin panel) and its question, over every plugin
/// window.
#[derive(Debug, Default)]
pub struct HostTier {
    /// The host's windows' presentation.
    pub ui: PluginUi,
    /// The question up, if any.
    pub asking: Asking,
    reply: Option<HostReply>,
}

impl HostTier {
    /// Where the host's windows among `views` were drawn last frame.
    #[must_use]
    pub fn rects(&self, views: &[WindowView]) -> Vec<Rect> {
        views
            .iter()
            .filter(|v| v.open && v.tier == Tier::Host)
            .filter_map(|v| self.ui.rect_of(&v.plugin, &v.window))
            .collect()
    }

    /// Draw the host's windows among `views`, each as any window is drawn: where each is.
    pub fn windows(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        views: &[WindowView],
    ) -> Vec<Rect> {
        self.ui.retain(p.art, views);
        views
            .iter()
            .filter(|v| v.open && v.tier == Tier::Host)
            .filter_map(|v| self.ui.window(p, ctx, v))
            .collect()
    }

    /// Draw `question`, or let the one that was up go: the player's reply is kept for
    /// [`Self::take_reply`]. Where the dialog is, while one is up.
    pub fn ask(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        question: Option<&HostQuestion>,
    ) -> Option<Rect> {
        match question {
            Some(q) => {
                if let Some(reply) = self.asking.frame(p, ctx, q) {
                    self.reply = Some(reply);
                }
                Some(self.asking.drawn.frame)
            }
            None => {
                self.asking.forget();
                None
            }
        }
    }

    /// The player's answer to the question since the last call.
    pub fn take_reply(&mut self) -> Option<HostReply> {
        self.reply.take()
    }
}
