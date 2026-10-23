//! Character templates are source files; played characters live in the local server database.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub version: u32,
    pub level: u32,
    pub attributes: [u32; 6],
    #[serde(default)]
    pub skills: Vec<Skill>,
    #[serde(default)]
    pub equipment: Vec<Equipment>,
}

impl Default for Template {
    fn default() -> Self {
        Self {
            version: 1,
            level: 1,
            attributes: [60; 6],
            skills: Vec::new(),
            equipment: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Skill {
    pub name: String,
    pub training: String,
    pub ranks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Equipment {
    pub weenie: u32,
    #[serde(default)]
    pub location: u32,
}

#[derive(Debug, Default)]
pub struct Characters {
    pub open: bool,
    pub name: String,
    pub names: Vec<String>,
    pub template: Template,
    pub message: String,
    pub error: Option<String>,
    pub skill_name: String,
    pub equipment_id: u32,
    pub root: PathBuf,
    pub schema: Option<Schema>,
}

/// Character names are also file names and console arguments.
pub fn valid_name(name: &str) -> bool {
    (3..=24).contains(&name.len())
        && name.trim() == name
        && name.chars().all(|c| c.is_ascii_alphabetic() || c == ' ')
        && !["CON", "PRN", "AUX", "NUL"].contains(&name.to_ascii_uppercase().as_str())
}

impl Characters {
    pub fn open(world: &Path) -> Self {
        let root = world.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut out = Self {
            root,
            name: "Builder".into(),
            ..Default::default()
        };
        out.refresh();
        if let Ok(name) = std::fs::read_to_string(out.root.join(".playtest/selected-character")) {
            if valid_name(&name) {
                out.name = name;
            }
        }
        if let Err(e) = out.load() {
            out.message = e;
        }
        out
    }

    pub fn refresh(&mut self) {
        self.names = std::fs::read_dir(self.root.join("characters"))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|e| {
                let p = e.path();
                (p.extension().and_then(|x| x.to_str()) == Some("json"))
                    .then(|| p.file_stem()?.to_str().map(str::to_owned))
                    .flatten()
            })
            .filter(|n| valid_name(n))
            .collect();
        self.names.sort();
    }

    pub fn path(&self) -> Result<PathBuf, String> {
        if !valid_name(&self.name) {
            return Err("Use 3 to 24 letters and spaces for the character name.".into());
        }
        Ok(self
            .root
            .join("characters")
            .join(format!("{}.json", self.name)))
    }

    pub fn load(&mut self) -> Result<(), String> {
        let path = self.path()?;
        self.template = if path.is_file() {
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?
        } else {
            Template::default()
        };
        if self.template.version != 1 {
            return Err("Unsupported character template version.".into());
        }
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), String> {
        let path = self.path()?;
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(&self.template).map_err(|e| e.to_string())?;
        std::fs::write(path, format!("{text}\n")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(self.root.join(".playtest")).map_err(|e| e.to_string())?;
        std::fs::write(self.root.join(".playtest/selected-character"), &self.name)
            .map_err(|e| e.to_string())?;
        self.refresh();
        Ok(())
    }

    pub fn applied(&self) -> PathBuf {
        self.root
            .join(".playtest")
            .join(format!("{}.applied", self.name))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: character template source and safe names).
    use super::*;
    #[test]
    fn character_names_cannot_escape_the_world_or_inject_console_commands() {
        assert!(valid_name("Test Mage"));
        for name in ["../escape", "A\nexit", "A\"B", "CON", " Test ", "ab"] {
            assert!(!valid_name(name), "{name:?}");
        }
    }
    #[test]
    fn templates_round_trip_without_account_or_inventory_object_ids() {
        let t = Template {
            level: 100,
            skills: vec![Skill {
                name: "WarMagic".into(),
                training: "Trained".into(),
                ranks: 200,
            }],
            equipment: vec![Equipment {
                weenie: 123,
                location: 0,
            }],
            ..Default::default()
        };
        let text = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Template>(&text).unwrap(), t);
        assert!(!text.contains("account"));
    }
    #[test]
    fn reopening_a_world_recovers_its_selected_template_without_touching_played_state() {
        let mut serial = 0;
        let dir = loop {
            let p = std::env::temp_dir().join(format!(
                "lyceum-character-template-{}-{serial}",
                std::process::id()
            ));
            match std::fs::create_dir(&p) {
                Ok(()) => break p,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => serial += 1,
                Err(e) => panic!("{e}"),
            }
        };
        let world = dir.join("world.toml");
        let mut c = Characters::open(&world);
        c.name = "Test Mage".into();
        c.template.level = 100;
        c.save().unwrap();
        let shard = dir.join(".playtest/shard.sqlite");
        std::fs::write(&shard, b"played character").unwrap();
        c.template.level = 200;
        c.save().unwrap();
        let reopened = Characters::open(&world);
        assert_eq!(reopened.name, "Test Mage");
        assert_eq!(reopened.template.level, 200);
        assert_eq!(reopened.names, ["Test Mage"]);
        assert_eq!(std::fs::read(shard).unwrap(), b"played character");
        assert!(!reopened.applied().exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[derive(Debug, Deserialize)]
pub struct Schema {
    pub version: u32,
    pub max_level: u32,
    pub skills: Vec<String>,
    pub locations: Vec<Location>,
}
#[derive(Debug, Deserialize)]
pub struct Location {
    pub name: String,
    pub value: u32,
}
