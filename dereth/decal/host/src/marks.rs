//! Markers as Decal's 3-D objects.
//!
//! The plugin host resolves every plugin's markers each frame; what Decal's 3-D service can draw is
//! sent as a [`Mark`]: a marker on an object (floating text when it has a label, else an arrow
//! over it) or at an outdoor place, a ring, text floating in the world, an object's nametag, and
//! the arrow, which Decal draws over the character's head pointing the way. Lines and squares in
//! the world, radar blips and map marks have no counterpart; nor has a place indoors, which has no
//! map coordinates for Decal to anchor to.
//!
//! An object's marker follows the object by itself, so a mark is sent again only when what it is
//! changes; a place's when it moves by a tenth of a meter or more.

use std::collections::BTreeMap;

use dereth_client_sdk::plugin_api::Value;
use dereth_client_sdk::plugins::overlay::{Kind, OverlayView, Overlays, ShapeKind};

/// The key the arrow's mark has: there is one arrow.
pub const ARROW: &str = "arrow";

/// What a mark is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Text floating over its object or place.
    Text,
    /// An arrow pointing down at its object or place.
    Arrow,
    /// A ring on the ground, `scale` meters in radius.
    Ring,
    /// The arrow over the character's head, toward its object or place.
    Pointer,
}

impl Look {
    const fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Arrow => "arrow",
            Self::Ring => "ring",
            Self::Pointer => "pointer",
        }
    }
}

/// One of Decal's 3-D objects.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub key: String,
    pub look: Look,
    /// The object it is on, or else its place: map coordinates north and east, and meters up.
    pub object: Option<u32>,
    pub place: Option<(f64, f64, f64)>,
    pub text: String,
    /// `0xRRGGBB`.
    pub color: u32,
    pub scale: f64,
}

impl Mark {
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut v = Value::map()
            .with("op", "mark")
            .with("key", self.key.as_str())
            .with("look", self.look.name())
            .with("text", self.text.as_str())
            .with("color", self.color)
            .with("scale", self.scale);
        if let Some(o) = self.object {
            v = v.with("object", o);
        }
        if let Some((ns, ew, z)) = self.place {
            v = v.with("ns", ns).with("ew", ew).with("z", z);
        }
        v
    }

    /// Whether `other` is the same mark, or one whose place moved too little to send.
    fn same(&self, other: &Self) -> bool {
        let near = match (self.place, other.place) {
            (Some(a), Some(b)) => {
                (a.0 - b.0).abs() * 240.0 < 0.1
                    && (a.1 - b.1).abs() * 240.0 < 0.1
                    && (a.2 - b.2).abs() < 0.1
            }
            (None, None) => true,
            _ => false,
        };
        near && Self {
            place: None,
            ..self.clone()
        } == Self {
            place: None,
            ..other.clone()
        }
    }
}

/// The mark `v` is drawn as, if Decal can draw it.
#[must_use]
pub fn of(v: &OverlayView) -> Option<Mark> {
    let place = || {
        let at = v.at.as_ref()?;
        let (ns, ew) = dereth_client_sdk::plugins::places::coords(at)?;
        Some((ns, ew, f64::from(at.frame.origin.z)))
    };
    let (look, key, scale) = match (v.kind, v.shape) {
        (Kind::World, _) => (
            if v.label.is_empty() {
                Look::Arrow
            } else {
                Look::Text
            },
            format!("{}/{}", v.plugin, v.id),
            1.0,
        ),
        (Kind::Nametag, _) => (
            Look::Text,
            format!("{}/tag/{}", v.plugin, v.object.map_or(0, |o| o.0)),
            1.0,
        ),
        (Kind::Shape, Some(ShapeKind::Text)) => (Look::Text, format!("{}/{}", v.plugin, v.id), 1.0),
        (Kind::Shape, Some(ShapeKind::Ring)) => {
            // The ring's radius, from its outline's first point.
            let r = match (v.path.first(), v.at.as_ref()) {
                (Some(p), Some(c)) => dereth_client_sdk::plugins::places::ground_distance(p, c),
                _ => 1.0,
            };
            (Look::Ring, format!("{}/{}", v.plugin, v.id), r)
        }
        (Kind::Arrow, _) => (Look::Pointer, ARROW.to_owned(), 1.0),
        _ => return None,
    };
    let object = v.object.map(|o| o.0);
    let place = if object.is_some() {
        None
    } else {
        Some(place()?)
    };
    Some(Mark {
        key,
        look,
        object,
        place,
        text: v.label.clone(),
        color: v.color,
        scale,
    })
}

/// The marks as last sent.
#[derive(Debug, Default)]
pub struct Marks {
    shown: BTreeMap<String, Mark>,
}

impl Marks {
    /// The ops that bring Decal's 3-D objects up to the markers as they stand.
    pub fn sync(&mut self, overlays: &Overlays, ops: &mut Vec<crate::Op>) {
        let mut now: BTreeMap<String, Mark> = BTreeMap::new();
        for v in &overlays.markers {
            if let Some(m) = of(v) {
                now.insert(m.key.clone(), m);
            }
        }
        for k in self.shown.keys() {
            if !now.contains_key(k) {
                ops.push(crate::Op::Unmark { key: k.clone() });
            }
        }
        for (k, m) in &now {
            if !self.shown.get(k).is_some_and(|s| s.same(m)) {
                ops.push(crate::Op::Mark(m.clone()));
            }
        }
        // A place that moved too little keeps the place it was sent at.
        for (k, m) in &mut now {
            if let Some(s) = self.shown.get(k) {
                if s.same(m) {
                    m.place = s.place;
                }
            }
        }
        self.shown = now;
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a host inside another program: markers as Decal's 3-D objects)
    use super::*;
    use dereth_client_sdk::plugins::places::from_coords;
    use dereth_client_sdk::primitives::ObjectId;

    fn view(id: i64, kind: Kind) -> OverlayView {
        OverlayView {
            plugin: "p".into(),
            id,
            kind,
            label: String::new(),
            color: 0x00FF_0000,
            icon: None,
            object: None,
            at: None,
            points: Vec::new(),
            line: false,
            shape: None,
            path: Vec::new(),
            width: 1,
            on_top: false,
            reach: None,
        }
    }

    fn sync(marks: &mut Marks, markers: Vec<OverlayView>) -> Vec<crate::Op> {
        let mut ops = Vec::new();
        marks.sync(
            &Overlays {
                player: None,
                markers,
            },
            &mut ops,
        );
        ops
    }

    #[test]
    fn a_marker_on_an_object_is_its_label_floating_over_it_or_else_an_arrow() {
        let mut named = view(1, Kind::World);
        named.object = Some(ObjectId(7));
        named.label = "Loot".into();
        let mut bare = view(2, Kind::World);
        bare.object = Some(ObjectId(8));
        let a = of(&named).expect("a mark");
        assert_eq!(
            (a.look, a.object, a.text.as_str()),
            (Look::Text, Some(7), "Loot")
        );
        assert_eq!(of(&bare).map(|m| m.look), Some(Look::Arrow));
    }

    #[test]
    fn a_line_or_a_marker_with_nowhere_to_be_has_no_mark() {
        let mut line = view(1, Kind::Shape);
        line.shape = Some(ShapeKind::Line);
        line.at = from_coords(10.0, 20.0, 0.0);
        assert_eq!(of(&line), None);
        let mut nowhere = view(2, Kind::World);
        nowhere.label = "x".into();
        assert_eq!(of(&nowhere), None, "no place and no object");
    }

    #[test]
    fn a_place_is_sent_again_only_when_it_moves_a_tenth_of_a_meter() {
        let mut marks = Marks::default();
        let at = |ns: f64| {
            let mut v = view(1, Kind::World);
            v.label = "here".into();
            v.at = from_coords(ns, 20.0, 0.0);
            v
        };
        assert_eq!(sync(&mut marks, vec![at(10.0)]).len(), 1);
        // A hundredth of a meter, then another: neither is sent, and they do not add up unsent.
        let step = 0.01 / 240.0;
        assert!(sync(&mut marks, vec![at(10.0 + step)]).is_empty());
        assert!(sync(&mut marks, vec![at(10.0 + 2.0 * step)]).is_empty());
        let moved = sync(&mut marks, vec![at(10.0 + 0.5 / 240.0)]);
        assert!(
            matches!(moved.as_slice(), [crate::Op::Mark(m)] if m.key == "p/1"),
            "{moved:?}"
        );
        assert_eq!(
            sync(&mut marks, Vec::new()),
            vec![crate::Op::Unmark { key: "p/1".into() }]
        );
    }
}
