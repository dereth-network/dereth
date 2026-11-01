//! The client's own preferences for the options window's client page.
//!
//! The graphics section's preferences are listed when the page asks for them, tried as the player
//! sets them, taken as applied when the page applies them and put back when it recalls them; after
//! each apply the overall graphics quality they match is worked out again. The client draws at the
//! highest quality, so its preferences start there (the original starts from its own defaults
//! unless the configuration program saved a quality) unless the settings say otherwise, and the
//! world is drawn by them once applied. The other sections (rendering, camera, avatars, input,
//! effects, sound, other) list nothing yet.

use auberean_draw::quality::{Graphics, Kind, GRAPHICS, PREFERENCES};
use auberean_ui::Token;
use auberean_ui_game::options::client::{Listing, Request, PREFERENCE_STRINGS};
use auberean_ui_game::options::helper::Change;
use auberean_ui_game::options::preference::{Choice, Description, Flag, Number};

/// The client's preferences as the client page sees them.
#[derive(Debug)]
pub struct Preferences {
    /// The graphics preferences as tried, and as last applied.
    now: Graphics,
    applied: Graphics,
    /// The page asks for the graphics section.
    shown: bool,
    listing: Option<Listing>,
    serial: u32,
    /// The overall graphics quality the applied preferences match (0: the player's own).
    pub quality: u32,
    /// Applied since the world last took them.
    fresh: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        let g = Graphics::first_run();
        Self {
            now: g.clone(),
            quality: g.overall(),
            applied: g,
            shown: false,
            listing: None,
            serial: 0,
            fresh: false,
        }
    }
}

impl Preferences {
    /// The preferences as last listed for the page.
    #[must_use]
    pub fn listing(&self) -> Option<&Listing> {
        self.listing.as_ref()
    }

    /// The graphics preferences the client starts with, as applied.
    pub fn set(&mut self, graphics: &Graphics) {
        self.now = graphics.clone();
        self.applied = graphics.clone();
        self.quality = self.applied.overall();
        self.relist();
    }

    /// The graphics preferences applied since the world last took them.
    pub fn take_applied(&mut self) -> Option<Graphics> {
        std::mem::take(&mut self.fresh).then(|| self.applied.clone())
    }

    /// One of the page's requests carried out.
    // D2-09722
    pub fn handle(&mut self, r: &Request) {
        match r {
            Request::Attach(sections) if sections.contains(&GRAPHICS) => {
                self.shown = true;
                self.relist();
            }
            Request::Detach(sections) if sections.contains(&GRAPHICS) => {
                self.shown = false;
                self.listing = None;
            }
            Request::Change(c) => {
                if let Some((key, v)) = value_of(c) {
                    self.now.set(key, v);
                }
            }
            Request::Apply(GRAPHICS) | Request::Synchronize(GRAPHICS) => {
                self.applied = self.now.clone();
                self.quality = self.applied.overall();
                self.fresh = true;
                tracing::debug!(quality = self.quality, "graphics preferences applied");
            }
            Request::Recall(GRAPHICS) => {
                self.now = self.applied.clone();
                self.relist();
            }
            _ => {}
        }
    }

    /// The graphics section listed again for the page.
    fn relist(&mut self) {
        if !self.shown {
            return;
        }
        self.serial += 1;
        let prefs = PREFERENCES
            .iter()
            .map(|p| listed(p.key, p, &self.now, &self.applied))
            .collect();
        self.listing = Some(Listing {
            serial: self.serial,
            sections: vec![(GRAPHICS, prefs)],
        });
    }
}

/// A graphics preference's key and value from a change the page made to it.
fn value_of(c: &Change) -> Option<(u32, f32)> {
    let (description, v) = match c {
        Change::Flag(f) => (f.description, f32::from(u8::from(f.get()))),
        Change::Number(n) => (n.description, n.get()),
        #[allow(clippy::cast_precision_loss)] // a few levels
        Change::Choice(ch) => (ch.description, ch.get() as f32),
    };
    let (section, key) = description.identify();
    (section == GRAPHICS).then_some((key, v))
}

/// A graphics preference as the page takes it: its label and help, its choices or range, its
/// default, the value last applied and the value now.
fn listed(
    key: u32,
    p: &auberean_draw::quality::Preference,
    now: &Graphics,
    applied: &Graphics,
) -> Change {
    let description = Description {
        section: GRAPHICS,
        key,
        table: PREFERENCE_STRINGS,
        label: Token::of(p.label).raw(),
        help: Token::of(p.help).raw(),
    };
    let (n, a, d) = (
        now.get(key).unwrap_or(0.0),
        applied.get(key).unwrap_or(0.0),
        now.default_of(key).unwrap_or(0.0),
    );
    let level = dereth_primitives::num::to_i32;
    match p.kind {
        Kind::Choice { choices, .. } => {
            let mut c = Choice::new();
            for (v, name) in choices {
                c.assign_choice(*v, (PREFERENCE_STRINGS, Token::of(name).raw()));
            }
            c.set_default(level(d));
            c.set_value(level(a));
            c.apply();
            c.set_value(level(n));
            c.description = description;
            Change::Choice(c)
        }
        Kind::Flag { .. } => {
            let mut f = Flag::new();
            f.set_default(d != 0.0);
            f.set_value(a != 0.0);
            f.apply();
            f.set_value(n != 0.0);
            f.description = description;
            Change::Flag(f)
        }
        Kind::Number {
            least, greatest, ..
        } => {
            let mut x = Number::new();
            x.set_range(least, greatest);
            x.set_default(d);
            x.set_value(a);
            x.apply();
            x.set_value(n);
            x.description = description;
            Change::Number(x)
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (how the client's own preferences reach the options window's client page)
    use super::*;
    use auberean_draw::quality::{key, FIRST_RUN};

    fn choice(l: &Listing, k: u32) -> Choice {
        l.sections[0]
            .1
            .iter()
            .find_map(|c| match c {
                Change::Choice(c) if c.description.key == k => Some(c.clone()),
                _ => None,
            })
            .expect("listed")
    }

    #[test]
    fn the_graphics_section_lists_tries_applies_and_recalls() {
        let mut p = Preferences::default();
        assert_eq!(p.quality, FIRST_RUN, "a fresh install's");
        assert!(p.listing().is_none(), "nothing listed until asked");
        p.handle(&Request::Attach(vec![0x4000_000B, GRAPHICS]));
        let l = p.listing().expect("listed").clone();
        assert_eq!(l.sections.len(), 1);
        assert_eq!(l.sections[0].1.len(), PREFERENCES.len());
        let texture = choice(&l, key::TEXTURE_DETAIL);
        assert_eq!(texture.get(), 3);
        assert_eq!(texture.list_choices().len(), 5);
        assert_eq!(texture.description.table, PREFERENCE_STRINGS);
        // A lower texture detail tried, then applied: no quality matches it.
        let mut lower = texture.clone();
        lower.set_value(2);
        p.handle(&Request::Change(Change::Choice(lower)));
        assert_eq!(p.quality, FIRST_RUN, "tried, not applied");
        p.handle(&Request::Apply(GRAPHICS));
        assert_eq!(p.quality, 0);
        assert_eq!(p.applied.get(key::TEXTURE_DETAIL), Some(2.0));
        // Tried back to high and recalled: the applied value listed again.
        let mut back = texture;
        back.set_value(3);
        p.handle(&Request::Change(Change::Choice(back)));
        p.handle(&Request::Recall(GRAPHICS));
        let again = p.listing().expect("listed").clone();
        assert!(again.serial > l.serial);
        assert_eq!(choice(&again, key::TEXTURE_DETAIL).get(), 2);
        p.handle(&Request::Detach(vec![GRAPHICS]));
        assert!(p.listing().is_none());
    }
}
