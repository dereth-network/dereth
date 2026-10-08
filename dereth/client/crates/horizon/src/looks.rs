//! How each character looked when it last stood in the world, remembered beside the settings so
//! that character select can show it: the server's list of characters says nothing of how they
//! look.
//!
//! One line a character: its key, a tab, then the look as space-separated fields.

use std::collections::BTreeMap;
use std::path::PathBuf;

use dereth_animation::parts::{AnimPartChange, ObjDesc, PaletteRange, TextureMapChange};
use dereth_primitives::DataId;

/// The file the looks are kept in, beside the interface's settings.
pub const FILE_NAME: &str = "horizon-looks.txt";

/// A character's look: what it is built from and how it is dressed.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// The setup it is built from.
    pub setup: DataId,
    /// What it wears and how it is coloured.
    pub desc: ObjDesc,
    /// Its heritage group, which places the camera.
    pub heritage: u32,
    /// How large it stands.
    pub scale: f32,
    /// Its level when it was last seen; 0 when not known (a look kept before levels were).
    pub level: u32,
    /// Its motion table, for its idle on character select; 0 when not known.
    pub motion_table: DataId,
}

/// The remembered looks, by character.
#[derive(Debug, Default)]
pub struct Looks {
    path: Option<PathBuf>,
    looks: BTreeMap<String, Look>,
}

/// The key a character's look is kept under: the server it plays on and its name.
#[must_use]
pub fn key(server: &str, name: &str) -> String {
    format!("{server}/{name}")
}

impl Looks {
    /// The looks kept in `path`, or none when there is no such file.
    #[must_use]
    pub fn load(path: Option<PathBuf>) -> Self {
        let looks = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| {
                text.lines()
                    .filter_map(|line| {
                        let (key, look) = line.split_once('\t')?;
                        Some((key.to_owned(), decode(look)?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { path, looks }
    }

    /// The look kept for `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Look> {
        self.looks.get(key)
    }

    /// Keep `look` for `key`, writing the file when it changed. False when the file could not
    /// be written.
    pub fn remember(&mut self, key: &str, look: Look) -> bool {
        if self.looks.get(key) == Some(&look) {
            return true;
        }
        self.looks.insert(key.to_owned(), look);
        let Some(path) = &self.path else {
            return true;
        };
        let text: String = self
            .looks
            .iter()
            .map(|(k, l)| format!("{k}\t{}\n", encode(l)))
            .collect();
        std::fs::write(path, text).is_ok()
    }
}

/// A look as one line's fields.
#[must_use]
pub fn encode(look: &Look) -> String {
    let mut fields = vec![
        format!("S:{:08X}", look.setup.0),
        format!("H:{}", look.heritage),
        format!("Z:{}", look.scale),
        format!("L:{}", look.level),
        format!("M:{:08X}", look.motion_table.0),
        format!("P:{:08X}", look.desc.palette_id.0),
    ];
    fields.extend(
        look.desc
            .part_changes
            .iter()
            .map(|c| format!("p:{}:{:08X}", c.part_index, c.part_id.0)),
    );
    fields.extend(look.desc.texture_changes.iter().map(|c| {
        format!(
            "t:{}:{:08X}:{:08X}",
            c.part_index, c.old_texture.0, c.new_texture.0
        )
    }));
    fields.extend(
        look.desc
            .subpalettes
            .iter()
            .map(|s| format!("s:{:08X}:{}:{}", s.palette_set.0, s.offset, s.length)),
    );
    fields.join(" ")
}

/// A look from one line's fields; `None` when they are not one.
#[must_use]
pub fn decode(text: &str) -> Option<Look> {
    let hex = |s: &str| u32::from_str_radix(s, 16).ok().map(DataId);
    let num = |s: &str| s.parse::<u32>().ok();
    let mut look = Look {
        setup: DataId(0),
        desc: ObjDesc::default(),
        heritage: 0,
        scale: 1.0,
        level: 0,
        motion_table: DataId(0),
    };
    for field in text.split_whitespace() {
        let parts: Vec<&str> = field.split(':').collect();
        match parts.as_slice() {
            ["S", v] => look.setup = hex(v)?,
            ["H", v] => look.heritage = num(v)?,
            ["Z", v] => look.scale = v.parse().ok()?,
            ["L", v] => look.level = num(v)?,
            ["M", v] => look.motion_table = hex(v)?,
            ["P", v] => look.desc.palette_id = hex(v)?,
            ["p", i, id] => look.desc.part_changes.push(AnimPartChange {
                part_index: num(i)?,
                part_id: hex(id)?,
            }),
            ["t", i, old, new] => look.desc.texture_changes.push(TextureMapChange {
                part_index: num(i)?,
                old_texture: hex(old)?,
                new_texture: hex(new)?,
            }),
            ["s", set, offset, length] => look.desc.subpalettes.push(PaletteRange {
                palette_set: hex(set)?,
                offset: num(offset)?,
                length: num(length)?,
            }),
            _ => return None,
        }
    }
    (look.setup.0 != 0).then_some(look)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    fn dressed() -> Look {
        Look {
            setup: DataId(0x0200_0001),
            desc: ObjDesc {
                part_changes: vec![AnimPartChange {
                    part_index: 16,
                    part_id: DataId(0x0100_4C3D),
                }],
                texture_changes: vec![TextureMapChange {
                    part_index: 9,
                    old_texture: DataId(0x0500_0CA4),
                    new_texture: DataId(0x0500_1D5E),
                }],
                palette_id: DataId(0x0400_007E),
                subpalettes: vec![PaletteRange {
                    palette_set: DataId(0x0F00_0001),
                    offset: 12,
                    length: 0,
                }],
            },
            heritage: 1,
            scale: 1.05,
            level: 101,
            motion_table: DataId(0x0900_0001),
        }
    }

    #[test]
    fn a_look_kept_in_the_file_comes_back_as_it_went_in() {
        let dir = std::env::temp_dir().join(format!("dereth-horizon-looks-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(FILE_NAME);
        let mut looks = Looks::load(Some(path.clone()));
        assert!(looks.remember(&key("127.0.0.1:9000", "+Aurelia Vale"), dressed()));
        let again = Looks::load(Some(path));
        assert_eq!(
            again.get(&key("127.0.0.1:9000", "+Aurelia Vale")),
            Some(&dressed())
        );
        assert_eq!(again.get(&key("elsewhere", "+Aurelia Vale")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_line_that_is_not_a_look_is_passed_over() {
        assert_eq!(decode("S:0200 Q:1"), None);
        assert_eq!(decode("H:1"), None, "a look with no setup is none");
        assert_eq!(decode(&encode(&dressed())), Some(dressed()));
    }
}
