//! A world's towns and named landmarks, and their drop points: where a portal to each leaves a
//! character.
//!
//! Names and drop points come from the world database itself, in two ways:
//!
//! 1. **Its named landmarks** (the places an operator may be sent to by name): each is where the
//!    portal it names leads.
//! 2. **The portals that lead outdoors**, named for where they lead ("Portal to Holtburg",
//!    "Holtburg Portal", "Town of Dryreach", or the place's bare name, "Danby's Outpost"): the name
//!    is the portal's less those words, and the drop point is where most of the portals of that
//!    name lead. A name names one place only when
//!    at least half the uses of its portals lead within [`SPREAD`] landblocks of that point; a
//!    name that does not ("Surface", "Exit") is passed over. Of two names whose drop points are
//!    within [`SPREAD`] landblocks of each other, a name shaped as a portal's to a place is kept over
//!    a bare one, then the one more portals lead to, and a named landmark is kept over both.
//!
//! A drop point outdoors with at least [`VENDORS`] vendors placed within a landblock of it is a
//! town; a named landmark that is not is kept as a landmark, and a name read from portals alone is
//! not kept. The vendors are what confirm a bare name: without them it is any portal's name.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::content::{Spot, Weenie, WorldSource};
use crate::documents::{destination, Place, PlaceKind, Portal};
use crate::requirement::Requirement;

/// How many vendors a drop point needs within a landblock of it to be a town.
pub const VENDORS: usize = 3;

/// How far apart, in landblocks, the drop points of one place may be.
pub const SPREAD: u32 = 2;

/// Whether a named place is a town.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TownKind {
    Town,
    Landmark,
}

/// A town or a named landmark, at its drop point.
#[derive(Debug, Clone, PartialEq)]
pub struct Town {
    pub kind: TownKind,
    pub name: String,
    /// Where a portal to it leaves a character.
    pub at: Spot,
    /// The vendors placed within a landblock of it.
    pub vendors: usize,
    /// Whether the name is one of the database's named landmarks.
    pub landmark: bool,
}

/// The landblocks' grid distance (the larger of the column and row distances).
fn apart(a: u32, b: u32) -> u32 {
    let (ax, ay) = (a >> 8, a & 0xFF);
    let (bx, by) = (b >> 8, b & 0xFF);
    ax.abs_diff(bx).max(ay.abs_diff(by))
}

/// The place a portal named `name` names: its name less "Portal to (the)", a trailing "Portal"
/// and a leading "Town of".
fn named(name: &str) -> Option<(String, bool)> {
    let mut n = name.trim();
    let mut shaped = false;
    for prefix in ["portal to the ", "portal to "] {
        if n.len() > prefix.len() && n[..prefix.len()].eq_ignore_ascii_case(prefix) {
            n = &n[prefix.len()..];
            shaped = true;
            break;
        }
    }
    let suffix = " portal";
    if n.len() > suffix.len() && n[n.len() - suffix.len()..].eq_ignore_ascii_case(suffix) {
        n = &n[..n.len() - suffix.len()];
        shaped = true;
    }
    let prefix = "town of ";
    if n.len() > prefix.len() && n[..prefix.len()].eq_ignore_ascii_case(prefix) {
        n = &n[prefix.len()..];
        shaped = true;
    }
    let n = n.trim();
    (!n.is_empty()).then(|| (n.to_owned(), shaped))
}

/// A drop point's key: its cell and its point to the decimetre.
fn key(at: &Spot) -> (u32, i32, i32, i32) {
    let d = |v: f32| dereth_primitives::num::to_i32((v * 10.0).round());
    (at.cell, d(at.origin[0]), d(at.origin[1]), d(at.origin[2]))
}

/// The towns and named landmarks of `content`, whose `portals` and `places` are read already;
/// `weenie` reads a weenie by class.
pub fn find(
    content: &dyn WorldSource,
    weenie: &mut dyn FnMut(u32) -> Option<Arc<Weenie>>,
    portals: &[Portal],
    places: &[Place],
) -> Vec<Town> {
    let mut vendors: HashMap<u32, usize> = HashMap::new();
    for p in places.iter().filter(|p| p.kind == PlaceKind::Vendor) {
        *vendors.entry(p.at.landblock()).or_default() += 1;
    }
    let near = |at: &Spot| -> usize {
        let b = at.landblock();
        let (x, y) = (b >> 8, b & 0xFF);
        let mut n = 0;
        for nx in x.saturating_sub(1)..=(x + 1).min(0xFF) {
            for ny in y.saturating_sub(1)..=(y + 1).min(0xFF) {
                n += vendors.get(&(nx << 8 | ny)).copied().unwrap_or(0);
            }
        }
        n
    };

    // The database's named landmarks, one per portal named (its fullest name).
    let mut marks: BTreeMap<u32, (String, Spot)> = BTreeMap::new();
    for l in content.landmarks() {
        let Some(w) = weenie(l.wcid) else { continue };
        let Some(at) = destination(&w, &Spot::at(0, [0.0; 3])) else {
            continue;
        };
        let name = l.name.trim().to_owned();
        let better = |old: &str| {
            let words = |s: &str| s.split_whitespace().count();
            (words(&name), name.len(), std::cmp::Reverse(name.clone()))
                > (words(old), old.len(), std::cmp::Reverse(old.to_owned()))
        };
        match marks.get(&l.wcid) {
            Some((old, _)) if !better(old) => {}
            _ => {
                marks.insert(l.wcid, (name, at));
            }
        }
    }

    // The names portals give the places they lead to outdoors.
    struct Named {
        name: String,
        shaped: bool,
        drops: BTreeMap<(u32, i32, i32, i32), (Spot, usize)>,
    }
    let mut names: BTreeMap<String, Named> = BTreeMap::new();
    for p in portals {
        if p.to.indoors() || p.requires.contains(&Requirement::Closed) {
            continue;
        }
        let Some((name, shaped)) = named(&p.name) else {
            continue;
        };
        let n = names.entry(name.to_lowercase()).or_insert_with(|| Named {
            name: name.clone(),
            shaped,
            drops: BTreeMap::new(),
        });
        n.shaped |= shaped;
        n.drops.entry(key(&p.to)).or_insert((p.to, 0)).1 += 1;
    }
    let mut candidates: Vec<(bool, usize, String, Spot)> = Vec::new();
    for n in names.into_values() {
        let uses: usize = n.drops.values().map(|(_, c)| c).sum();
        let Some((at, _)) = n
            .drops
            .values()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| key(&b.0).cmp(&key(&a.0))))
        else {
            continue;
        };
        let there: usize = n
            .drops
            .values()
            .filter(|(s, _)| apart(s.landblock(), at.landblock()) <= SPREAD)
            .map(|(_, c)| c)
            .sum();
        if there * 2 < uses {
            continue;
        }
        candidates.push((n.shaped, there, n.name, *at));
    }
    // The strongest first: names shaped as a portal's to a place, then the most portals leading
    // there, then by name.
    candidates.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)).then_with(|| a.2.cmp(&b.2)));

    let mut out: Vec<Town> = Vec::new();
    for (name, at) in marks.into_values() {
        let vendors = near(&at);
        let kind = if !at.indoors() && vendors >= VENDORS {
            TownKind::Town
        } else {
            TownKind::Landmark
        };
        out.push(Town {
            kind,
            name,
            at,
            vendors,
            landmark: true,
        });
    }
    for (_, _, name, at) in candidates {
        let taken = out.iter().any(|t| {
            !t.at.indoors() && apart(t.at.landblock(), at.landblock()) <= SPREAD
                || t.name.eq_ignore_ascii_case(&name)
        });
        let vendors = near(&at);
        if taken || vendors < VENDORS {
            continue;
        }
        out.push(Town {
            kind: TownKind::Town,
            name,
            at,
            vendors,
            landmark: false,
        });
    }
    out.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| key(&a.at).cmp(&key(&b.at)))
    });
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (how a world's town names are read from its portals).
    use super::named;

    #[test]
    fn a_portals_name_names_the_place_it_leads_to() {
        for (portal, place, shaped) in [
            ("Portal to Holtburg", "Holtburg", true),
            ("Portal to the Beach Fort", "Beach Fort", true),
            ("Holtburg Portal", "Holtburg", true),
            ("Town of Dryreach", "Dryreach", true),
            ("Portal To Hebian-To", "Hebian-To", true),
            ("Surface", "Surface", false),
        ] {
            assert_eq!(named(portal), Some((place.to_owned(), shaped)), "{portal}");
        }
        assert_eq!(named("Portal"), Some(("Portal".to_owned(), false)));
        assert_eq!(named("  "), None);
    }
}
