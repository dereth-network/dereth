//! A creature's table: the AC2 body it is made from, how large it is drawn, and which of AC2's
//! clips plays for each of Asheron's Call's motions. Ids and numbers only; the clips themselves
//! come from the player's own Asheron's Call 2 install.

use serde::Deserialize;

/// One Asheron's Call motion and the AC2 clip played for it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    /// The motion's name in the client's command list (`Ready`, `WalkForward`, `AttackHigh1`).
    pub command: String,
    /// The one stance the row plays in, `peace` or `combat`: a row for both when left out. A
    /// stance's own row takes the place of the row for both (a combat Ready, its fighting idle).
    pub stance: Option<String>,
    /// The AC2 keyframe clip played for it.
    pub animation: u32,
    /// How much faster than AC2 plays it (1 when left out). Ignored for a forward motion, whose
    /// pace comes from how far the body moves (`stride`).
    pub rate: Option<f64>,
    /// For a walk or a run: meters of AC2's size one round of the clip carries the body (AC2's
    /// own speed for the clip times its length). Measured from the feet when left out, which
    /// undercounts a body that hovers or shuffles.
    pub stride: Option<f64>,
    /// For a turn: radians a second, turning right (negative is clockwise, as the files hold it).
    /// AC2's own standing turn rate for the body when left out.
    pub omega: Option<f64>,
    /// For an attack: where the blow lands, as a share of the clip; AC2's own hit when left out,
    /// else the middle.
    pub hit: Option<f64>,
    /// For an attack: the combat table's row it is chosen by.
    pub attack: Option<Attack>,
}

/// The combat table's row an attack is chosen by.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attack {
    /// `high`, `medium` or `low`.
    pub height: String,
    /// `punch`, `kick`, `thrust` or `slash`: how the server asks for it.
    pub kind: String,
    /// The body part the blow is dealt with, by the server's body part number (0 head, 20 claw,
    /// 22 breath).
    pub part: u32,
    /// How far the blow reaches, meters.
    pub reach: f64,
}

/// A creature's table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub version: u32,
    /// Its name on a command line.
    pub name: String,
    /// Its name as a person reads it.
    pub label: String,
    /// The AC2 entity whose body and skin it is drawn with.
    pub entity: u32,
    /// The clip whose first keys the body is bound in.
    pub bind: u32,
    /// The share of AC2's size its walk and run are paced for (1 when left out): the size at
    /// which they carry it at Asheron's Call's walking and running speeds. The records are at
    /// AC2's own size, and a world draws the creature larger or smaller by its weenie's scale;
    /// the walk carries it as far as its feet step at any size, so drawn at another size it
    /// covers the ground faster or slower than at this one.
    pub paced_at: Option<f64>,
    /// The largest side of its pictures, texels.
    pub picture: u32,
    /// The icon its weenies show: a picture of the client's (`0x06…`). Until the family's own is
    /// drawn, the icon of the Asheron's Call creature type nearest it.
    pub icon: Option<u32>,
    /// How much farther than AC2 each coarser level of detail starts
    /// ([`DETAIL_DISTANCE_SCALE`](super::make::DETAIL_DISTANCE_SCALE) when left out).
    pub detail_distance_scale: Option<f64>,
    pub motion: Vec<Motion>,
    /// Its other looks: the same body under other pictures.
    #[serde(default)]
    pub look: Vec<Look>,
}

/// Another look of the body: an AC2 entity drawn with the same body, whose appearance lays other
/// pictures on it. A weenie draws it by swapping the body's textures for the look's.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    /// Its name on a command line.
    pub name: String,
    /// The AC2 entity it is drawn as.
    pub entity: u32,
}

impl Table {
    /// The table in `text`.
    ///
    /// # Errors
    /// The text is not a creature table.
    pub fn parse(text: &str) -> Result<Self, String> {
        let t: Self = toml::from_str(text).map_err(|e| format!("the creature table: {e}"))?;
        if t.version != 1 {
            return Err(format!(
                "creature table version {} (this reads 1)",
                t.version
            ));
        }
        if t.motion.is_empty() {
            return Err("the table names no motions".to_owned());
        }
        for m in &t.motion {
            if let Some(s) = &m.stance {
                if s != "peace" && s != "combat" {
                    return Err(format!(
                        "{}: {s} is not a stance (peace or combat)",
                        m.command
                    ));
                }
            }
        }
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 creature tables)
    use super::*;

    /// The tables kept beside the tool.
    fn kept() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ac2-creatures");
        let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
            .expect("the tables' folder")
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read_to_string(e.path()).expect("a table reads"),
                )
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn every_kept_table_reads_and_names_only_the_clients_motions() {
        let tables = kept();
        assert!(tables.len() >= 12, "{} tables", tables.len());
        for (file, text) in tables {
            let t = Table::parse(&text).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(format!("{}.toml", t.name), file);
            for m in &t.motion {
                assert!(
                    dereth_animation::MotionCommand::from_name(&m.command).is_some(),
                    "{file}: {} is not a motion",
                    m.command
                );
            }
            for needed in ["Ready", "WalkForward", "RunForward", "AttackHigh1", "Dead"] {
                assert!(
                    t.motion.iter().any(|m| m.command == needed),
                    "{file} has no {needed}"
                );
            }
            // Each look is another entity under its own name.
            let names: std::collections::BTreeSet<_> = t.look.iter().map(|l| &l.name).collect();
            let entities: std::collections::BTreeSet<_> =
                t.look.iter().map(|l| l.entity).chain([t.entity]).collect();
            assert_eq!(names.len(), t.look.len(), "{file} repeats a look's name");
            assert_eq!(entities.len(), t.look.len() + 1, "{file} repeats an entity");
        }
    }

    #[test]
    fn a_table_of_another_version_or_with_no_motions_is_refused() {
        let base =
            "version = 1\nname = \"x\"\nlabel = \"X\"\nentity = 1\nbind = 2\npicture = 512\n";
        assert!(Table::parse(base).is_err(), "no motions");
        let one = format!("{base}[[motion]]\ncommand = \"Ready\"\nanimation = 3\n");
        assert!(Table::parse(&one).is_ok());
        assert!(Table::parse(&one.replace("version = 1", "version = 2")).is_err());
    }

    #[test]
    fn a_row_names_its_stance_as_peace_or_combat() {
        let base =
            "version = 1\nname = \"x\"\nlabel = \"X\"\nentity = 1\nbind = 2\npicture = 512\n";
        let row = |stance: &str| {
            format!("{base}[[motion]]\ncommand = \"Ready\"\nstance = \"{stance}\"\nanimation = 3\n")
        };
        let t = Table::parse(&row("combat")).expect("a combat row");
        assert_eq!(t.motion[0].stance.as_deref(), Some("combat"));
        assert!(Table::parse(&row("peace")).is_ok());
        assert!(Table::parse(&row("angry")).is_err());
    }
}
