//! The story's acts, their scenes and the scenes' keyframes, as scripts hand them to the quest
//! window's story page and to the player of a scene.
//!
//! - **A keyframe** is one step of a scene: the scene it belongs to, its number in the scene, how
//!   long it lasts (a 64-bit float), a video, an audio and a voice track (each a data id), a
//!   subtitle (a string reference) and a movie (a file name), with a bit for each track it defines
//!   (1 video, 2 audio, 4 voice, 8 movie, 16 subtitle). A new one is all zero and empty; its
//!   duration is not set (zero here). Clearing it leaves its duration alone too.
//! - **A scene** has its id, its name, its act, its number in the act, whether it is hidden
//!   (locked) and whether it may be played, and its keyframes in order. A keyframe added to a scene
//!   takes the scene's id and the next number (one more than the scene holds).
//! - **An act** has its id, its number in the story, its name, whether it is visible and whether
//!   it may be played (the player holds one of its scenes), and its scenes ordered by their
//!   numbers.
//!
//! Each packs its fields in a fixed order (an act's number after its name), its list as a count
//! and the items. Lists of acts and of scenes are kept by value: putting one in copies it.

use crate::native_object::TextInfo;
use crate::wire::{In, Out, Result};

/// A keyframe's track bits.
pub mod track {
    pub const VIDEO: u32 = 0x1;
    pub const AUDIO: u32 = 0x2;
    pub const VOICE: u32 = 0x4;
    pub const MOVIE: u32 = 0x8;
    pub const SUBTITLE: u32 = 0x10;
}

/// One step of a scene. Dropping one lets go of its texts.
// D2-12339
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keyframe {
    pub scene: u32,
    pub number: u32,
    /// A 64-bit float's bits, seconds.
    pub duration: u64,
    pub video: u32,
    pub audio: u32,
    pub voice: u32,
    pub subtitle: TextInfo,
    pub movie: Vec<u16>,
    /// The tracks it defines ([`track`]).
    pub tracks: u32,
}

impl Keyframe {
    /// Everything but its duration back to a new one's.
    // D2-12336
    pub fn clear(&mut self) {
        let duration = self.duration;
        *self = Self {
            duration,
            ..Self::default()
        };
    }

    /// Its tracks' data emptied (the bits and the duration kept), as a scene handed to a player
    /// who may not see it is.
    // D2-12337
    pub fn clear_for_hackers(&mut self) {
        self.video = 0;
        self.audio = 0;
        self.voice = 0;
        self.subtitle = TextInfo::default();
        self.movie.clear();
    }

    /// Whether it defines a track.
    // D2-12321, D2-12322, D2-12323, D2-12324
    #[must_use]
    pub const fn has(&self, track: u32) -> bool {
        self.tracks & track != 0
    }

    /// Whether its duration is set (anything but -1).
    // D2-12325
    #[must_use]
    pub fn has_duration(&self) -> bool {
        f64::from_bits(self.duration) != -1.0
    }

    /// Read a packed one.
    // D2-12342
    pub fn read(r: &mut In<'_>) -> Result<Self> {
        Ok(Self {
            scene: r.u32()?,
            number: r.u32()?,
            duration: r.u64()?,
            video: r.u32()?,
            audio: r.u32()?,
            voice: r.u32()?,
            subtitle: TextInfo::read(r, 0)?,
            movie: r.wide()?,
            tracks: r.u32()?,
        })
    }

    /// The packed form.
    // D2-12342
    pub fn write(&self, w: &mut Out) {
        w.u32(self.scene);
        w.u32(self.number);
        w.u64(self.duration);
        w.u32(self.video);
        w.u32(self.audio);
        w.u32(self.voice);
        self.subtitle.write(w);
        w.wide(&self.movie);
        w.u32(self.tracks);
    }
}

/// A scene of an act. A copy holds a copy of each of its steps, in order.
// D2-12306, D2-12299, D2-12303
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneInfo {
    pub id: u32,
    pub name: TextInfo,
    pub act: u32,
    pub number: u32,
    /// Hidden (locked).
    pub hidden: u32,
    pub playable: u32,
    pub keyframes: Vec<Keyframe>,
}

impl SceneInfo {
    /// Back to a new one's.
    // D2-12291, D2-12298
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Every keyframe's tracks emptied.
    // D2-12287
    pub fn clear_for_hackers(&mut self) {
        self.keyframes
            .iter_mut()
            .for_each(Keyframe::clear_for_hackers);
    }

    /// A keyframe added at the end: a copy, with this scene's id and the next number. Answers 1.
    // D2-12289, D2-12284
    pub fn add_keyframe(&mut self, k: &Keyframe) -> u32 {
        let mut k = k.clone();
        k.scene = self.id;
        #[allow(clippy::cast_possible_truncation)]
        let n = self.keyframes.len() as u32;
        k.number = n.wrapping_add(1);
        self.keyframes.push(k);
        1
    }

    /// Read a packed one.
    // D2-12283, D2-12294, D2-12295
    pub fn read(r: &mut In<'_>) -> Result<Self> {
        let id = r.u32()?;
        let name = TextInfo::read(r, 0)?;
        let act = r.u32()?;
        let number = r.u32()?;
        let hidden = r.u32()?;
        let playable = r.u32()?;
        let n = r.u32()? as usize;
        let n = r.count(n, 4)?;
        let mut keyframes = Vec::with_capacity(n);
        for _ in 0..n {
            keyframes.push(Keyframe::read(r)?);
        }
        Ok(Self {
            id,
            name,
            act,
            number,
            hidden,
            playable,
            keyframes,
        })
    }

    /// The packed form.
    // D2-12283, D2-12293, D2-00265
    pub fn write(&self, w: &mut Out) {
        w.u32(self.id);
        self.name.write(w);
        w.u32(self.act);
        w.u32(self.number);
        w.u32(self.hidden);
        w.u32(self.playable);
        write_count(w, self.keyframes.len());
        self.keyframes.iter().for_each(|k| k.write(w));
    }
}

/// An act of the story. A copy holds a copy of each of its scenes, in order.
// D2-12319, D2-12311, D2-12314
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActInfo {
    pub id: u32,
    pub number: u32,
    pub name: TextInfo,
    pub visible: u32,
    /// The player holds one of its scenes.
    pub playable: u32,
    /// Ordered by their numbers.
    pub scenes: Vec<SceneInfo>,
}

impl ActInfo {
    /// Filled: its id, name, number, whether visible and playable, and its scenes (`ordered`: as
    /// given, replacing what it held; else each added after those it holds with a number no
    /// greater).
    // D2-12315, D2-12312
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        &mut self,
        id: u32,
        name: &TextInfo,
        number: u32,
        visible: u32,
        playable: u32,
        scenes: &[SceneInfo],
        ordered: bool,
    ) {
        self.id = id;
        self.name = name.clone();
        self.visible = visible;
        self.number = number;
        self.playable = playable;
        if ordered {
            self.scenes = scenes.to_vec();
        } else {
            for s in scenes {
                self.add_scene(s);
            }
        }
    }

    /// A scene added after those it holds with a number no greater. Answers 1.
    // D2-12309
    pub fn add_scene(&mut self, s: &SceneInfo) -> u32 {
        insert_sorted(&mut self.scenes, s.clone(), |x| x.number)
    }

    /// Read a packed one.
    // D2-12308
    pub fn read(r: &mut In<'_>) -> Result<Self> {
        let id = r.u32()?;
        let name = TextInfo::read(r, 0)?;
        let number = r.u32()?;
        let visible = r.u32()?;
        let playable = r.u32()?;
        let scenes = read_scenes(r)?;
        Ok(Self {
            id,
            number,
            name,
            visible,
            playable,
            scenes,
        })
    }

    /// The packed form.
    // D2-12308
    pub fn write(&self, w: &mut Out) {
        w.u32(self.id);
        self.name.write(w);
        w.u32(self.number);
        w.u32(self.visible);
        w.u32(self.playable);
        write_scenes(w, &self.scenes);
    }
}

/// An item put in a list ordered by a key: after every item whose key is no greater. Answers 1.
// D2-01578, D2-01650
pub fn insert_sorted<T>(list: &mut Vec<T>, item: T, key: impl Fn(&T) -> u32) -> u32 {
    let k = key(&item);
    let at = list.iter().position(|x| key(x) > k).unwrap_or(list.len());
    list.insert(at, item);
    1
}

fn write_count(w: &mut Out, n: usize) {
    #[allow(clippy::cast_possible_truncation)]
    w.u32(n as u32);
}

/// A packed list of scenes: the count, then each.
// D2-00266, D2-00267
pub fn read_scenes(r: &mut In<'_>) -> Result<Vec<SceneInfo>> {
    let n = r.u32()? as usize;
    let n = r.count(n, 4)?;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(SceneInfo::read(r)?);
    }
    Ok(v)
}

/// A list of scenes' packed form.
pub fn write_scenes(w: &mut Out, scenes: &[SceneInfo]) {
    write_count(w, scenes.len());
    scenes.iter().for_each(|s| s.write(w));
}

/// A packed list of acts: the count, then each.
// D2-00777, D2-00778
pub fn read_acts(r: &mut In<'_>) -> Result<Vec<ActInfo>> {
    let n = r.u32()? as usize;
    let n = r.count(n, 4)?;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(ActInfo::read(r)?);
    }
    Ok(v)
}

/// A list of acts' packed form.
// D2-15139, D2-00776
pub fn write_acts(w: &mut Out, acts: &[ActInfo]) {
    write_count(w, acts.len());
    acts.iter().for_each(|a| a.write(w));
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (packed forms and list order on hand-built values)
    use super::*;

    fn scene(id: u32, number: u32) -> SceneInfo {
        SceneInfo {
            id,
            number,
            ..SceneInfo::default()
        }
    }

    #[test]
    fn an_act_with_scenes_and_keyframes_writes_back_exactly() {
        let mut s = scene(7, 2);
        s.name.string_id = 3;
        s.add_keyframe(&Keyframe {
            duration: 2.5f64.to_bits(),
            movie: "m.bik".encode_utf16().collect(),
            tracks: track::MOVIE,
            ..Keyframe::default()
        });
        let a = ActInfo {
            id: 1,
            number: 4,
            visible: 1,
            scenes: vec![s],
            ..ActInfo::default()
        };
        let mut w = Out::new();
        write_acts(&mut w, std::slice::from_ref(&a));
        let mut r = In::new(&w.buf);
        let back = read_acts(&mut r).unwrap();
        assert!(r.at_end());
        assert_eq!(back, vec![a]);
        assert_eq!(back[0].scenes[0].keyframes[0].scene, 7);
        assert_eq!(back[0].scenes[0].keyframes[0].number, 1);
    }

    #[test]
    fn an_item_goes_after_those_whose_number_is_no_greater() {
        let mut v = Vec::new();
        for (id, n) in [(1, 2), (2, 1), (3, 2), (4, 0)] {
            insert_sorted(&mut v, scene(id, n), |s| s.number);
        }
        let ids: Vec<u32> = v.iter().map(|s| s.id).collect();
        assert_eq!(ids, [4, 2, 1, 3]);
    }

    #[test]
    fn clearing_a_keyframe_keeps_its_duration() {
        let mut k = Keyframe {
            scene: 1,
            duration: 3.0f64.to_bits(),
            tracks: track::VIDEO,
            video: 9,
            ..Keyframe::default()
        };
        k.clear_for_hackers();
        assert_eq!((k.video, k.tracks), (0, track::VIDEO));
        k.clear();
        assert_eq!(k.duration, 3.0f64.to_bits());
        assert_eq!(k.tracks, 0);
        assert!(k.has_duration());
    }
}
