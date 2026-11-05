//! Tooltips: once the pointer has rested on an element that shows tooltips for a while (a quarter
//! of a second unless the player chose otherwise), and no button is held, the element's tooltip
//! template from its layout is made with the element's tip text in it, sized to the text, and put
//! in front of everything just below and right of the cursor, kept on the screen. It goes when the
//! pointer moves to another element, when a button is pressed, or after ten seconds.

use crate::system::{Handle, Ui};
use crate::token::{attr, Token};
use crate::types::button_id;

/// The tooltip shown, and when tooltips show.
#[derive(Clone, Debug)]
pub struct Tooltips {
    /// Whether tooltips show at all.
    pub enable: bool,
    /// Seconds the pointer rests before one shows, and seconds one stays.
    pub delay: f32,
    pub duration: f32,
    /// The tooltip shown, and when it showed.
    pub shown: Option<Handle>,
    pub since: f64,
}

impl Default for Tooltips {
    fn default() -> Self {
        Self {
            enable: true,
            delay: 0.25,
            duration: 10.0,
            shown: None,
            since: -1.0,
        }
    }
}

impl Ui {
    /// The tooltip shown, if any.
    #[must_use]
    pub fn tooltip(&self) -> Option<Handle> {
        self.tips.shown.filter(|h| self.node(*h).is_some())
    }

    /// The clock's part in tooltips: with none shown, one shows once the pointer has rested for
    /// the delay; one shown goes after its duration.
    pub(crate) fn tooltip_use_time(&mut self) {
        let now = self.now();
        if self.tooltip().is_none() {
            if f64::from(self.tips.delay) <= now - self.mgr.last_move {
                self.start_tooltip();
            }
        } else if f64::from(self.tips.duration) <= now - self.tips.since {
            self.clear_tooltip();
        }
    }

    /// Show the tooltip of the element the pointer rests on: when tooltips are on and the left
    /// button is not held, the element's `tooltip_ID` template from its layout is made, the text
    /// (the element's own tip, a string reference given to the text element as one, else its
    /// `tooltip_entry` string) put in the template's
    /// `tooltip_text_ID` element, the template sized so that element holds the text (as tall as
    /// the text runs), and put in front of everything at the pointer past the cursor, kept on the
    /// screen.
    // D2-11210
    pub fn start_tooltip(&mut self) {
        self.clear_tooltip();
        if !self.tips.enable {
            return;
        }
        let Some(owner) = self.mgr.tooltip_owner.filter(|h| self.node(*h).is_some()) else {
            return;
        };
        if self.button_held(button_id::LEFT) {
            return;
        }
        let Some(n) = self.node(owner) else { return };
        let Some(template) = n.id_attr(attr::TOOLTIP_ID) else {
            return;
        };
        let info = n.tooltip_info.clone();
        let words = match (&info, &n.tooltip_text) {
            // Measured as it writes now (the client's error message while a table is missing).
            (Some(i), _) => Some(String::from_utf16_lossy(&self.assets().element_text(i))),
            (None, Some(t)) => Some(t.clone()),
            (None, None) => n.string_id_attr(attr::TOOLTIP_ENTRY).and_then(|tok| {
                let table = self.string_table(owner);
                self.assets().string(table, tok)
            }),
        };
        let Some(words) = words else { return };
        let layout = self.library().get(n.refdesc).layout;
        let Some(tip) = self.create(layout, Token(template)) else {
            return;
        };
        self.add_root(tip);
        let Some(field) = self
            .node(tip)
            .and_then(|n| n.id_attr(attr::TOOLTIP_TEXT_ID))
            .and_then(|id| self.child_recursive(tip, Token(id)))
        else {
            self.remove(tip);
            return;
        };
        let Some((w, h)) = self.inq_size_with_margins(field, &words) else {
            self.remove(tip);
            return;
        };
        let (Some(tr), Some(fr)) = (
            self.node(tip).map(|n| n.rect),
            self.node(field).map(|n| n.rect),
        ) else {
            return;
        };
        let (pad_w, pad_h) = (tr.width() - fr.width(), tr.height() - fr.height());
        self.resize_to(tip, w + pad_w, h + pad_h);
        match info {
            Some(i) => {
                self.set_string_info(field, &i);
            }
            None => self.set_text(field, &words),
        }
        self.recalculate_glyph_list(field);
        let paper = self.text_state(field).map_or(0, |t| t.paper_h);
        let tall = if h < paper { paper } else { h };
        let width = self.node(tip).map_or(0, |n| n.rect.width());
        self.resize_to(tip, width, tall + pad_h);
        let (cw, ch) = self
            .cursor()
            .and_then(|c| self.assets().image(c.picture))
            .map_or((0, 0), |img| {
                (
                    i32::try_from(img.width).unwrap_or(0),
                    i32::try_from(img.height).unwrap_or(0),
                )
            });
        let (px, py) = self.pointer();
        let (dw, dh) = self.display();
        let (tw, th) = self
            .node(tip)
            .map_or((0, 0), |n| (n.rect.width(), n.rect.height()));
        let x = (px + cw).max(0).min(dw - tw);
        let y = (py + ch).max(0).min(dh - th);
        self.move_to(tip, x, y);
        self.bring_root_to_front(tip);
        self.tips.shown = Some(tip);
        self.tips.since = self.now();
    }

    /// Take the tooltip away.
    // D2-11200
    pub fn clear_tooltip(&mut self) {
        if let Some(t) = self.tips.shown.take() {
            self.remove(t);
        }
    }

    /// Show the tooltip again (its text changed).
    // D2-11223
    pub fn reset_tooltip(&mut self) {
        self.clear_tooltip();
        self.start_tooltip();
    }

    /// Whether tooltips show at all, as the player set it.
    #[must_use]
    pub const fn tooltips_show(&self) -> bool {
        self.tips.enable
    }

    /// The player's tooltip settings: the delay before one shows, and whether they show (either
    /// change redoes the tooltip shown).
    // D2-11226
    pub fn set_tooltip_preferences(&mut self, delay: Option<f32>, enable: Option<bool>) {
        let now = self.now();
        if let Some(d) = delay {
            self.tips.delay = d;
            if self.tooltip().is_none() {
                if f64::from(d) <= now - self.mgr.last_move {
                    self.start_tooltip();
                }
            } else if f64::from(self.tips.duration) <= now - self.tips.since {
                self.clear_tooltip();
            }
        }
        if let Some(e) = enable {
            self.tips.enable = e;
            self.clear_tooltip();
            self.start_tooltip();
        }
    }

    /// An element's own tip text, in place of its `tooltip_entry`; shown again when its tooltip
    /// is up.
    // D2-11312
    pub fn set_tooltip(&mut self, h: Handle, words: &str) {
        if let Some(n) = self.node_mut(h) {
            n.tooltip_text = Some(words.to_owned());
            n.tooltip_info = None;
        }
        if self.mgr.tooltip_owner == Some(h) && self.tooltip().is_some() {
            self.reset_tooltip();
        }
    }

    /// An element's own tip as a string reference, in place of its `tooltip_entry`: the
    /// tooltip's text element is given it, and writes it once the string tables it reads are at
    /// hand. Shown again when its tooltip is up.
    // D2-11312
    pub fn set_tooltip_info(&mut self, h: Handle, info: &auberean_text::StringInfo) {
        if let Some(n) = self.node_mut(h) {
            n.tooltip_info = Some(Box::new(info.clone()));
            n.tooltip_text = None;
        }
        if self.mgr.tooltip_owner == Some(h) && self.tooltip().is_some() {
            self.reset_tooltip();
        }
    }

    /// An element's own tip text, when one was set.
    #[must_use]
    pub fn element_tooltip(&self, h: Handle) -> Option<&str> {
        self.node(h)?.tooltip_text.as_deref()
    }

    /// Drop an element's own tip text.
    // D2-11326
    pub fn clear_element_tooltip(&mut self, h: Handle) {
        if let Some(n) = self.node_mut(h) {
            n.tooltip_text = None;
            n.tooltip_info = None;
        }
        if self.mgr.tooltip_owner == Some(h) && self.tooltip().is_some() {
            self.reset_tooltip();
        }
    }

    /// Whether a pointer button is down on any element.
    fn button_held(&self, b: u32) -> bool {
        fn walk(ui: &Ui, h: Handle, b: u32) -> bool {
            ui.node(h)
                .is_some_and(|n| n.holds(b) || n.children.iter().any(|c| walk(ui, *c, b)))
        }
        self.roots().iter().any(|r| walk(self, *r, b))
    }
}
