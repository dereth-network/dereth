//! What an effect aimed at another object does to that object: it may hide it for as long as the
//! effect plays, take it up at a holding location, play an effect of its own on it (stopped with
//! this one), let go of it, or destroy it when it is one the client made. An effect may make an
//! object: it then aims at the object it made, which goes when the effect ends. The rest of the
//! effect plays on its own object as any other.

use auberean_assets::fx::FxNode;
use auberean_physics::collision::{settings, CollisionSettings};
use auberean_physics::world::MOVER_ETHEREAL;

use crate::{Client, Scene};

/// A second property of the physics type an object an effect makes passes through.
const MADE_OBJECT_ETHEREAL: u32 = 0x4000_0026;
/// The slider an object an effect makes fades in on, over its entry's ramp time, else this long.
const MADE_OBJECT_FADE_SLIDER: u32 = 0x4000_001A;
const MADE_OBJECT_FADE_IN: f32 = 0.25;
/// How long an object an effect made fades out when the effect ends, and how long until it goes.
const MADE_OBJECT_FADE: f64 = 0.5;
const MADE_OBJECT_GONE: f64 = 3.0;
/// How long an object an effect makes as a clone of another lasts, seconds.
const CLONE_SECONDS: f64 = 5.0;

/// An effect playing on an object and aimed at another.
#[derive(Debug)]
pub(crate) struct Aimed {
    object: u64,
    effect: u32,
    target: u64,
    /// The objects it hid, to show again when it ends.
    hidden: Vec<u64>,
    /// The effects it plays on its target, to stop when it ends.
    target_fx: Vec<u32>,
    /// The objects it made, to fade out and destroy when it ends.
    made: Vec<u64>,
    /// Whether it was seen playing, and how many frames it has been looked for.
    seen: bool,
    age: u32,
}

impl Scene<'_> {
    /// Object `object` plays `effect` at strength `scalar`, aimed at object `target` (an aim taken
    /// up with what it is asked this frame, [`Scene::aim_fx`]).
    // D2-06163, D2-06497
    pub(crate) fn do_targeted_fx(&mut self, object: u64, effect: u32, scalar: f32, target: u64) {
        self.fx_aims.push((object, effect, target));
        self.do_fx(object, effect, scalar);
    }

    /// The objects this frame's effects make and what they do to what they aim at: an effect that
    /// is already playing only takes its strength. Each entry played where the object is now, for
    /// who it is and the time of day, makes its object and acts on its target.
    // D2-06907, D2-06916, D2-06932, D2-06547
    pub(crate) fn aim_fx(&mut self, client: &mut Client, played: &[(u64, u32, f32)], night: bool) {
        let aims = std::mem::take(&mut self.fx_aims);
        for &(object, effect, scalar) in played {
            let target = aims
                .iter()
                .find(|(o, e, _)| *o == object && *e == effect)
                .map_or(0, |a| a.2);
            if self.fx_is_playing(object, effect) {
                continue;
            }
            let mut aimed = Aimed {
                object,
                effect,
                target,
                hidden: Vec::new(),
                target_fx: Vec::new(),
                made: Vec::new(),
                seen: false,
                age: 0,
            };
            let player = self.player.as_ref().is_some_and(|p| p.id == object);
            for n in self.fx_nodes(object, effect) {
                if self.systems.fx_channels().mismatch(&n)
                    || (n.player_only == Some(true) && !player)
                    || (n.day_time == Some(false) && !night)
                    || (n.night_time == Some(false) && night)
                {
                    continue;
                }
                if let Some(look) = n.visual_desc.filter(|v| !v.is_none()) {
                    self.do_create_object_fx(client, &mut aimed, look.0, &n);
                }
                if aimed.target != 0 || n.retarget == Some(true) {
                    self.do_target_fx(client, &mut aimed, &n, scalar);
                }
            }
            if !aimed.hidden.is_empty() || !aimed.target_fx.is_empty() || !aimed.made.is_empty() {
                self.aimed.push(aimed);
            }
        }
    }

    /// Whether `object` plays `effect` now.
    fn fx_is_playing(&mut self, object: u64, effect: u32) -> bool {
        self.fx_of(object)
            .is_some_and(|f| f.fx.states().contains_key(&effect))
    }

    /// `object`'s effects, whichever kind of object it is.
    pub(crate) fn fx_of(&mut self, object: u64) -> Option<&mut crate::fx::BodyFx> {
        match self.player.as_mut() {
            Some(p) if p.id == object => p.effects.as_mut(),
            _ => self
                .others
                .effects_mut(object)
                .or_else(|| self.held.effects_mut(object).map(|(fx, _)| fx))
                .or_else(|| {
                    self.temporaries
                        .get_mut(object)
                        .and_then(|t| t.effects.as_mut())
                }),
        }
    }

    /// The entries of `object`'s effect `effect` that it plays where it is now.
    fn fx_nodes(&mut self, object: u64, effect: u32) -> Vec<FxNode> {
        let Some(fx) = self.fx_of(object) else {
            return Vec::new();
        };
        let (table, terrain) = (fx.fx.table().clone(), fx.fx.terrain());
        table
            .fx_data(&self.world.records, effect)
            .iter()
            .map(|d| auberean_draw::effects::node_for(d, terrain).clone())
            .collect()
    }

    /// What one entry of an aimed effect does to its target.
    fn do_target_fx(&mut self, client: &mut Client, a: &mut Aimed, n: &FxNode, scalar: f32) {
        if n.hide_target == Some(true) {
            if let Some(b) = self.body_mut(a.target) {
                b.set_no_draw(true);
                a.hidden.push(a.target);
            }
        }
        let mut target = a.target;
        if n.retarget == Some(true) && n.conditional != Some(true) {
            let clone = self.body(a.object).map_or(0, |b| b.look_to_clone);
            if clone == 0 {
                return;
            }
            let Some(made) = self.create_temporary(client, clone, CLONE_SECONDS) else {
                return;
            };
            self.set_made_object(a.object, made);
            target = made;
        }
        if !self.has_body(target) {
            return;
        }
        if let (Some(location), Some(true)) = (n.holding_location, n.parent) {
            if self.temporaries.has(target) {
                self.hold_temporary(target, a.object, location);
            }
        }
        if let Some(fx) = n.target_fx.filter(|f| *f != 0) {
            self.do_fx(target, fx, scalar);
            a.target_fx.push(fx);
        }
        if n.unparent == Some(true) {
            if let Some(b) = self.body_mut(target) {
                b.unset_parent();
            }
        }
        // D2-06885
        if n.destroy_target == Some(true) && self.temporaries.has(target) {
            self.destroy_temporary(client, target);
        }
    }

    /// An object an effect makes: the effect aims at it; it passes through what objects made this
    /// way do, fades in, and hangs from the object playing it at the entry's holding location or
    /// bone, or stands where that object does (on the water, for an entry that says so). An entry
    /// naming a behavior has it play and destroy the object as it ends; otherwise, or if it can't
    /// play, the object goes when the effect ends.
    // D2-06052
    fn do_create_object_fx(&mut self, client: &mut Client, a: &mut Aimed, look: u32, n: &FxNode) {
        let Some(made) = self.create_temporary(client, look, -1.0) else {
            return;
        };
        self.set_made_object(a.object, made);
        a.target = made;
        if let Some(f) = self.temporaries.figure_mut(made) {
            let ramp = n.ramp_time.unwrap_or(0.0);
            let fade = if ramp == 0.0 {
                MADE_OBJECT_FADE_IN
            } else {
                ramp
            };
            f.sliders.do_slider(0, MADE_OBJECT_FADE_SLIDER, 0.0, 0.0);
            f.sliders.do_slider(0, MADE_OBJECT_FADE_SLIDER, 1.0, fade);
        }
        let (location, bone) = (n.holding_location.unwrap_or(0), n.bone.unwrap_or(0));
        if location != 0 || bone != 0 {
            self.fx_hold_temporary(
                client,
                made,
                a.object,
                (location, bone),
                crate::pv(n.bone_offset.unwrap_or(auberean_primitives::Vec3::ZERO)),
                n.remove_rotation == Some(true),
            );
        } else if let Some(mut at) = self.body(a.object).map(|b| b.position) {
            if n.water_plane == Some(true) {
                if let Some(h) = crate::water_height(&self.land, at.cell) {
                    at.frame.origin.z = h;
                }
            }
            let (land, table) = (&self.land, &mut self.table);
            if let Some(b) = self.temporaries.body_mut(made) {
                b.enter_world_at(land, table, &at, false);
            }
        }
        let plays = n.behavior.filter(|b| *b != 0).is_some_and(|behavior| {
            self.play_behavior(
                made,
                &auberean_animation::track::Params {
                    behavior,
                    destroy_on_completion: true,
                    context: 1,
                    ..auberean_animation::track::Params::default()
                },
            )
        });
        if !plays {
            a.made.push(made);
        }
    }

    /// `object`'s effects now aim at `made`, an object they made, which passes through what such
    /// an object does.
    fn set_made_object(&mut self, object: u64, made: u64) {
        if let Some(b) = self.body_mut(object) {
            b.set_fx_target(made);
        }
        let list = self.table.property_list();
        let mut pass = auberean_physics::ethereal::PhysicsTypes::default();
        pass.set_property(list, MADE_OBJECT_ETHEREAL, true);
        pass.set_property(list, MOVER_ETHEREAL, true);
        let s = CollisionSettings {
            bits: settings::PHYSICS_TYPE,
            physics_type: pass,
            ..CollisionSettings::default()
        };
        self.set_body_collision_state(made, &s);
    }

    /// A slider an effect moved on an object that moves those of what it holds too, each object it
    /// holds taking it.
    // D2-06233
    pub(crate) fn do_sliders_on_children(&mut self) {
        let mut ids: Vec<u64> = self.player.iter().map(|p| p.id).collect();
        ids.extend(self.others.ids());
        ids.extend(self.held.ids());
        ids.extend(self.temporaries.ids().into_iter().map(|(id, _)| id));
        for id in ids {
            let sliders = match self.fx_of(id) {
                Some(f) if !f.child_sliders.is_empty() => std::mem::take(&mut f.child_sliders),
                _ => continue,
            };
            let children: Vec<u64> = self
                .held
                .holders()
                .into_iter()
                .filter(|(_, holder)| *holder == id)
                .map(|(child, _)| child)
                .chain(self.temporaries.held_by(id))
                .collect();
            for child in children {
                if let Some(f) = self.figure_mut(child) {
                    for s in &sliders {
                        f.sliders.do_slider(0, s.slider, s.value, s.time);
                    }
                }
            }
        }
    }

    /// Aimed effects that have ended show again what they hid and stop what they played on their
    /// targets. An effect that never got to play, or ended within a frame, ends after two frames.
    // D2-06920
    pub(crate) fn end_aimed_fx(&mut self, now: f64) {
        let mut aimed = std::mem::take(&mut self.aimed);
        aimed.retain_mut(|a| {
            let playing = self.fx_is_playing(a.object, a.effect);
            a.seen |= playing;
            a.age += 1;
            if playing || (!a.seen && a.age < 2) {
                return true;
            }
            for &h in &a.hidden {
                if let Some(b) = self.body_mut(h) {
                    b.set_no_draw(false);
                }
            }
            for &fx in &a.target_fx {
                self.stop_fx(a.target, fx);
            }
            for &m in &a.made {
                self.temporaries
                    .fade_and_remove(m, now, MADE_OBJECT_FADE, MADE_OBJECT_GONE);
            }
            false
        });
        self.aimed = aimed;
    }
}
