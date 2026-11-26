//! The damage texts the scripts sent this frame, the objects that left the world this frame, and
//! where each object a damage text stands over is on the screen.
//!
//! The scripts decide which hits are shown, by the player's options: none while the damage
//! numbers are off, and others' hits only while the other characters' damage numbers are on and
//! within their range. Whatever they send is shown.

use std::collections::{BTreeMap, BTreeSet};

use auberean_client_sdk::contract::notice::{HitText, Notice, Said, WorldNotice};
use auberean_client_sdk::contract::view::View;

use crate::draw::Rect;

/// The damage texts and the objects they stand over.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Damage {
    /// The damage texts the scripts sent this frame, in order, each with the object it stands
    /// over.
    pub texts: Vec<(u64, HitText)>,
    /// The objects that left the world this frame.
    pub gone: Vec<u64>,
    /// The box each object a damage text stands over fills on the screen, as the last world frame
    /// drew it; an object none of whose box was drawn on the screen has none.
    pub at: BTreeMap<u64, Rect>,
    /// The objects in the world as the view last gave them.
    known: BTreeSet<u64>,
}

impl Damage {
    /// Follow one frame: the objects that left the view, and the scripts' damage texts.
    pub fn follow(&mut self, view: Option<&View>, notices: &[Notice]) {
        self.texts.clear();
        self.gone.clear();
        let now: BTreeSet<u64> = view
            .and_then(|v| v.world.as_ref())
            .map(|s| s.things.iter().map(|t| t.id).collect())
            .unwrap_or_default();
        self.gone.extend(self.known.difference(&now));
        self.known = now;
        for n in notices {
            if let (Said::World(WorldNotice::Damage(Some(h))), Some(o)) = (&n.said, n.about) {
                self.texts.push((o, h.clone()));
            }
        }
    }
}
