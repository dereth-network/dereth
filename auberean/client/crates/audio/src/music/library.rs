//! The music files by name, as the music's own references name them, and everything a piece needs
//! to play gathered before it plays.
//!
//! Every music file lives in a music record (portal `0x24`); the data's file table names each such
//! record by its file name, and a file names another by that name (in any letter case). A piece's
//! needs are the pieces it starts, the styles it plays, the collections its bands take instruments
//! from and the scripts it calls, each with its own needs.

use std::collections::BTreeMap;
use std::sync::Arc;

use auberean_assets::sound::Music;
use auberean_assets::tables::FileIdTable;
use auberean_assets::{decode, Context};

use super::dls::{self, Collection};
use super::form::{self, Reference, Script, Segment, Style, TrackKind};
use crate::records::SoundFiles;

/// The file table's record.
pub const FILE_TABLE: u32 = 0x0E00_0003;

/// A music file, read.
#[derive(Clone, Debug)]
pub enum Object {
    Segment(Arc<Segment>),
    Style(Arc<Style>),
    Collection(Arc<Collection>),
    Script(Arc<Script>),
    /// A file of a form the player does not use (an audio path configuration).
    Other([u8; 4]),
}

/// A piece and everything it needs, by lowercase file name.
#[derive(Clone, Debug)]
pub struct Piece {
    pub id: u32,
    pub segment: Arc<Segment>,
    pub needs: BTreeMap<String, Object>,
}

impl Piece {
    /// What a reference names, when the piece gathered it.
    #[must_use]
    pub fn get(&self, r: &Reference) -> Option<&Object> {
        self.needs.get(&r.file.as_deref()?.to_lowercase())
    }
}

/// The music files known by name, and those read so far.
#[derive(Debug, Default)]
pub struct Library {
    names: BTreeMap<String, u32>,
    objects: BTreeMap<u32, Object>,
}

impl Library {
    /// The names of the music records, from the data's file table (none when it is missing).
    #[must_use]
    pub fn new(files: &dyn SoundFiles) -> Self {
        let mut names = BTreeMap::new();
        if let Some(t) = files
            .read(FILE_TABLE)
            .and_then(|b| decode::<FileIdTable>(FILE_TABLE, &b, &Context::default()).ok())
        {
            for ty in &t.types {
                for e in &ty.entries {
                    if e.id.0 >> 24 == 0x24 {
                        names.insert(e.name.to_lowercase(), e.id.0);
                    }
                }
            }
        }
        Self {
            names,
            objects: BTreeMap::new(),
        }
    }

    /// The record a file name names.
    // D2-07865
    #[must_use]
    pub fn id_of(&self, file: &str) -> Option<u32> {
        self.names.get(&file.to_lowercase()).copied()
    }

    /// How many music files the file table names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the file table named no music files.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// A music record, read (and kept).
    // D2-07850
    ///
    /// # Errors
    /// The record is missing, or its file does not read.
    pub fn load(&mut self, files: &dyn SoundFiles, id: u32) -> Result<Object, String> {
        if let Some(o) = self.objects.get(&id) {
            return Ok(o.clone());
        }
        let bytes = files
            .read(id)
            .ok_or_else(|| format!("no music record {id:08X}"))?;
        let m: Music =
            decode(id, &bytes, &Context::default()).map_err(|e| format!("{id:08X}: {e}"))?;
        let form = m
            .bytes
            .get(8..12)
            .map_or([0; 4], |f| [f[0], f[1], f[2], f[3]]);
        let o = match &form {
            b"DMSG" => Object::Segment(Arc::new(
                form::segment(&m.bytes).map_err(|e| format!("{id:08X}: {e}"))?,
            )),
            b"DMST" => Object::Style(Arc::new(
                form::style(&m.bytes).map_err(|e| format!("{id:08X}: {e}"))?,
            )),
            b"DLS " => Object::Collection(Arc::new(
                dls::parse(&m.bytes).map_err(|e| format!("{id:08X}: {e}"))?,
            )),
            b"DMSC" => Object::Script(Arc::new(
                form::script(&m.bytes).map_err(|e| format!("{id:08X}: {e}"))?,
            )),
            f => Object::Other(*f),
        };
        self.objects.insert(id, o.clone());
        Ok(o)
    }

    /// A file by name, read.
    ///
    /// # Errors
    /// No such name, or its file does not read.
    pub fn load_named(&mut self, files: &dyn SoundFiles, file: &str) -> Result<Object, String> {
        let id = self
            .id_of(file)
            .ok_or_else(|| format!("no music file named {file}"))?;
        self.load(files, id)
    }

    /// A piece and everything it needs.
    // D2-07854
    ///
    /// # Errors
    /// The piece, or anything it names, is missing or does not read.
    pub fn piece(&mut self, files: &dyn SoundFiles, id: u32) -> Result<Piece, String> {
        let Object::Segment(segment) = self.load(files, id)? else {
            return Err(format!("{id:08X} is not a piece"));
        };
        let mut needs = BTreeMap::new();
        self.gather(files, &segment, &mut needs)?;
        Ok(Piece { id, segment, needs })
    }

    fn need(
        &mut self,
        files: &dyn SoundFiles,
        r: &Reference,
        needs: &mut BTreeMap<String, Object>,
    ) -> Result<(), String> {
        let Some(file) = r.file.as_deref() else {
            return Ok(());
        };
        let key = file.to_lowercase();
        if needs.contains_key(&key) {
            return Ok(());
        }
        let o = self.load_named(files, file)?;
        needs.insert(key, o.clone());
        match o {
            Object::Segment(s) => self.gather(files, &s, needs)?,
            Object::Style(st) => {
                for b in &st.bands {
                    for i in &b.instruments {
                        if let Some(c) = &i.collection {
                            self.need(files, c, needs)?;
                        }
                    }
                }
            }
            Object::Script(sc) => {
                for obj in &sc.objects {
                    if let Some(s) = &obj.segment {
                        self.gather(files, s, needs)?;
                    }
                    if let Some(r) = &obj.reference {
                        self.need(files, r, needs)?;
                    }
                }
            }
            Object::Collection(_) | Object::Other(_) => {}
        }
        Ok(())
    }

    fn gather(
        &mut self,
        files: &dyn SoundFiles,
        s: &Segment,
        needs: &mut BTreeMap<String, Object>,
    ) -> Result<(), String> {
        for t in &s.tracks {
            match &t.kind {
                TrackKind::Band(items) => {
                    for b in items {
                        for i in &b.band.instruments {
                            if let Some(c) = &i.collection {
                                self.need(files, c, needs)?;
                            }
                        }
                    }
                }
                TrackKind::Trigger(items) => {
                    for i in items {
                        self.need(files, &i.segment, needs)?;
                    }
                }
                TrackKind::Style(items) => {
                    for (_, r) in items {
                        self.need(files, r, needs)?;
                    }
                }
                TrackKind::Script(calls) => {
                    for c in calls {
                        self.need(files, &c.script, needs)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}
