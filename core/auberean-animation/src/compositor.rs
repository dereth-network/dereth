//! Blending the animations a body plays into one pose: per layer, then layer over layer.
//!
//! Every playing behavior adds its animation to its layer's blender, weighted by the behavior's
//! weight (and, while the body changes mode, by how far the change is: the mode being left weighs
//! `1 - t`, the one entered `t`). Layers blend from the highest down. A normal layer is blended
//! apart and then fills whatever weight its bones still have free; a "tweak" layer adds straight
//! into the pose. Within a normal layer each animation counts as one whole pose per bone, except
//! those of a tracker that plays the moving behavior, which add by their own bone weights.

use auberean_primitives::DataId;

use crate::clip::{Pose, Sum};
use crate::table::Table;

/// One animation in a layer: what plays, how much it weighs, which tracker plays it and where
/// its time is (in cycles).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub animation: DataId,
    pub weight: f32,
    /// The tracker: 0 the mode being left (or the only one), 1 the mode being entered.
    pub tracker: usize,
    pub time: f32,
}

/// One layer's animations.
// D2-14814, D2-14816
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Blender {
    pub entries: Vec<Entry>,
}

/// What a blender asks to play an entry: add `entry` at `weight` into `out`, and whether the
/// entry's tracker plays the moving behavior.
pub trait Player {
    fn play(&mut self, entry: &Entry, weight: f32, out: &mut Pose);
    fn playing_move(&self, tracker: usize) -> bool;
}

impl Blender {
    /// Keep the first `n` entries.
    // D2-14817
    pub fn set_count(&mut self, n: usize) {
        self.entries.truncate(n);
    }

    // D2-14818
    pub fn add(&mut self, entry: Entry) {
        self.entries.push(entry);
    }

    /// Blend the layer into `out` at `layer_weight`. A normal layer plays each entry alone and
    /// adds it per bone either as a whole pose (scaled by the entry's weight over the bone's own
    /// weight, the bone counting the entry's weight) or, for a moving tracker, as it is (the bone
    /// counting the entry's weight times its own). A tweak layer plays each entry straight into
    /// `out`. When the entries weigh more than 1 in all, the pose is scaled back: only the first
    /// bone, as the client does.
    // D2-14815
    pub fn animate(&self, tweak: bool, layer_weight: f32, out: &mut Pose, player: &mut dyn Player) {
        if self.entries.is_empty() {
            return;
        }
        let n = out.bones.len();
        let mut temp = Pose::new(n);
        for e in &self.entries {
            if tweak {
                player.play(e, layer_weight * e.weight, out);
                continue;
            }
            temp.clear();
            let w = layer_weight * e.weight;
            player.play(e, 1.0, &mut temp);
            let moving = player.playing_move(e.tracker);
            for b in 0..n {
                let s = temp.bones[b];
                let tw = temp.weights[b];
                if moving {
                    out.bones[b].add(s.offset, s.scale, s.rotation, w);
                    out.weights[b] += w * tw;
                } else {
                    out.bones[b].add(s.offset, s.scale, s.rotation, w / tw);
                    out.weights[b] += w;
                }
            }
        }
        let total: f32 = self.entries.iter().map(|e| layer_weight * e.weight).sum();
        if total > 1.0 && n > 0 {
            let k = 1.0 / total;
            let s = out.bones[0];
            out.bones[0] = Sum::ZERO;
            out.bones[0].add(s.offset, s.scale, s.rotation, k);
            out.weights[0] *= k;
        }
    }
}

/// A body part's layers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Compositor {
    /// Sliders whose value weighs a layer.
    pub slider_layers: Table<u32>,
    pub layer_weights: Vec<f32>,
    pub layers: Vec<Blender>,
    /// Per layer, from the highest: whether it adds straight into the pose.
    pub tweaks: Vec<i32>,
    pub bones: usize,
}

impl Compositor {
    /// A compositor of `layers` layers over `bones` bones; `tweaks` lists, from the lowest layer
    /// up, which add straight into the pose. Nothing changes when the counts are the same.
    // D2-14822, D2-14825, D2-14824, D2-14819
    pub fn init(&mut self, layers: usize, tweaks: &[i32], bones: usize) {
        if layers == self.layers.len() && bones == self.bones {
            return;
        }
        self.layers = vec![Blender::default(); layers];
        self.layer_weights = vec![1.0; layers];
        self.tweaks = vec![0; layers];
        for i in 0..layers {
            self.tweaks[layers - 1 - i] = tweaks.get(i).copied().unwrap_or(0);
        }
        self.bones = bones;
    }

    /// Empty every layer, each back at full weight.
    // D2-14820
    pub fn clear(&mut self) {
        for (b, w) in self.layers.iter_mut().zip(&mut self.layer_weights) {
            b.set_count(0);
            *w = 1.0;
        }
    }

    /// Add an animation to layer `layer` (a layer past the last is ignored).
    // D2-14821
    pub fn add(&mut self, layer: u32, entry: Entry) {
        if let Some(b) = self.layers.get_mut(layer as usize) {
            b.add(entry);
        }
    }

    /// Blend every layer into `out`, from the highest down; `slider` reads the sliders that
    /// weigh layers.
    // D2-14823
    pub fn animate(
        &mut self,
        out: &mut Pose,
        slider: &dyn Fn(u32) -> f32,
        player: &mut dyn Player,
    ) {
        for (s, layer) in self.slider_layers.iter() {
            if let Some(w) = self.layer_weights.get_mut(*layer as usize) {
                *w = slider(s);
            }
        }
        let n = out.bones.len();
        let mut temp = Pose::new(n);
        for layer in (0..self.layers.len()).rev() {
            let blender = &self.layers[layer];
            if blender.entries.is_empty() {
                continue;
            }
            let lw = self.layer_weights[layer];
            if self.tweaks.get(layer).copied().unwrap_or(0) != 0 {
                blender.animate(true, lw, out, player);
                continue;
            }
            temp.clear();
            blender.animate(false, lw, &mut temp, player);
            for b in 0..n {
                let (have, got) = (out.weights[b], temp.weights[b]);
                if have >= 1.0 {
                    continue;
                }
                if have == 0.0 {
                    out.weights[b] = got;
                    out.bones[b] = temp.bones[b];
                    continue;
                }
                let free = 1.0 - have;
                let (k, add) = if free <= got {
                    (free / got, free)
                } else {
                    (1.0, got)
                };
                let s = temp.bones[b];
                out.bones[b].add(s.offset, s.scale, s.rotation, k);
                out.weights[b] += add;
            }
        }
    }
}
