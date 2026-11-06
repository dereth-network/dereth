//! The objects the client makes itself: the missiles of the stories.
//!
//! A temporary object takes the lowest temporary id free and is made from a record (an entity, or
//! a visual description) as its figure, with a physics body whose collision spheres its setup
//! gives. It has no game object, but it is among the client's objects, its timeout its lifetime;
//! when the client lets it go (asked to destroy it), the scene lets it go too. It is in the world's
//! table but not in the world until it is placed, and it plays its look's startup effects from its
//! making. Made with a timeout, it fades
//! out over half a second when the timeout has passed and goes half a second later; removed at once
//! when its story ends with it unfired.
//!
//! A missile held before it is fired is taken up by its holder's body at a holding location and
//! stands each frame where it hangs from its holder's figure. Fired, its guidance places it (where
//! it is held, or where its aim says it starts) and launches it at its next physics tick; it flies
//! by its guidance until it strikes something, which fades it out over half a second and removes it
//! then, or until its timeout fades it. It tells no one when its life or its fuel runs out.

use std::collections::BTreeMap;

use auberean_animation::body::Body as Figure;
use auberean_animation::host::Completion;
use auberean_animation::CachedRecords;
use auberean_assets::fx::AppearanceKey;
use auberean_client_sdk::protocol::types::VisualDesc as WireVisual;
use auberean_client_sdk::Client;
use auberean_draw::body::BodyDrawer;
use auberean_draw::scene::ObjectBatch;
use auberean_physics::body::parent::{set_parent, sticky_set_parent, TakeUp, FADE_SLIDER};
use auberean_physics::body::{setup_spheres, Body, BodyConfig, Notice, TICK};
use auberean_physics::collision::CollisionSettings;
use auberean_physics::missile::{MissileParameters, MissileTargetingParameters};
use auberean_physics::motion::values::default_values;
use auberean_physics::world::NoGameObjects;
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::look::{Desc, Look, ObjectLook};
use auberean_world::math::Mat4;
use auberean_world::place::PlaceRecords;
use auberean_world::records::WorldRecords;
use dereth_primitives::Position;

use crate::{Scene, BLOCK_LENGTH};

/// The part group an appearance laid over an effect's clone goes to.
const CLONE_APPEARANCE_GROUP: u32 = 0x4000_0002;
/// How long a fading object takes to fade out, and the opacity it fades to, seconds.
pub const FADE_TIME: f64 = 0.5;
/// How long after a behavior asks that its object go it goes, hidden until then, seconds.
pub(crate) const REMOVED_GONE: f64 = 3.0;

/// One object the client made.
pub(crate) struct Temporary {
    /// The record it was made from.
    pub data_id: u32,
    /// The body whose story made it (0 for none).
    pub maker: u64,
    /// Its own descriptions, once the server sent it one (none while its look is its record's).
    pub dress: Option<ObjectLook>,
    pub figure: Option<Figure>,
    pub body: Body,
    pub scale: Vec3,
    /// Whether it has stood in the world, and so has a place of its own: what is only ever held
    /// has none.
    pub stood: bool,
    /// The holder and the holding location it is taken up at, while it is held.
    pub holder: Option<(u64, u32)>,
    /// When its timeout passes (the client's time), when it has one.
    pub expires: Option<f64>,
    /// When it goes, once it is fading out.
    pub removal: Option<f64>,
    /// The effects it plays.
    pub effects: Option<crate::fx::BodyFx>,
    /// Where it was put in the world, until it has gone a meter from there.
    pub placed_at: Option<Position>,
}

/// The objects the client made, by id.
#[derive(Default)]
pub(crate) struct Temporaries {
    made: BTreeMap<u64, Temporary>,
}

/// The look a record makes: an entity's object (its scale taken with it), or a visual
/// description itself.
fn look_of_record(records: &WorldRecords, data_id: u32) -> Option<Look> {
    let mut id = data_id;
    let mut scale = Vec3::new(1.0, 1.0, 1.0);
    for _ in 0..8 {
        match id >> 24 {
            0x47 => {
                let e = records.entity(DataId(id))?;
                if let Some(s) = e.scale {
                    scale = Vec3::new(scale.x * s.x, scale.y * s.y, scale.z * s.z);
                }
                id = e.object?;
            }
            0x1F => {
                let mut look = Look::of_record(records, DataId(id)).ok()?;
                let s = look.scale;
                look.scale = Vec3::new(scale.x * s.x, scale.y * s.y, scale.z * s.z);
                return Some(look);
            }
            _ => return None,
        }
    }
    None
}

/// A temporary object's body: the collision spheres its look's setup has, still otherwise.
fn body_of(id: u64, figure: Option<&Figure>, scale: f32, at: Position) -> Body {
    let still = default_values();
    let spheres = figure
        .and_then(|f| f.parts.first())
        .map(|p| setup_spheres(&p.setup))
        .unwrap_or_default();
    let mut b = Body::new(
        BodyConfig {
            spheres,
            step_up: 0.0,
            step_down: 0.0,
            scale,
            ground: still,
            air: still,
        },
        at,
        0.0,
    );
    b.id = id;
    b.is_player = false;
    // Made, it is not in the world until it is placed.
    b.in_world = false;
    b
}

impl Temporaries {
    /// The decals the bodies' effects laid since last asked, each with where its body stands.
    pub(crate) fn take_decals(
        &mut self,
        out: &mut Vec<(Position, auberean_draw::effects::FxDecal)>,
    ) {
        for b in self.made.values_mut() {
            if let Some(fx) = b.effects.as_mut() {
                let place = b.body.position;
                out.extend(fx.fx.take_decals().into_iter().map(|d| (place, d)));
            }
        }
    }

    /// The sounds every body's effects played or stopped since last taken, each with its object.
    pub(crate) fn take_sounds(&mut self, out: &mut Vec<(u64, auberean_draw::effects::FxSound)>) {
        for fx in self.made.values_mut().filter_map(|b| b.effects.as_mut()) {
            out.extend(fx.sounds.drain(..).map(|s| (fx.id, s)));
        }
    }

    pub(crate) fn has(&self, id: u64) -> bool {
        self.made.contains_key(&id)
    }

    pub(crate) fn get(&self, id: u64) -> Option<&Temporary> {
        self.made.get(&id)
    }

    pub(crate) fn get_mut(&mut self, id: u64) -> Option<&mut Temporary> {
        self.made.get_mut(&id)
    }

    pub(crate) fn figure_mut(&mut self, id: u64) -> Option<&mut Figure> {
        self.made.get_mut(&id)?.figure.as_mut()
    }

    /// The objects held by `holder`.
    pub(crate) fn held_by(&self, holder: u64) -> Vec<u64> {
        self.made
            .iter()
            .filter(|(_, t)| t.holder.is_some_and(|(h, _)| h == holder))
            .map(|(id, _)| *id)
            .collect()
    }

    /// What every object's body asked since last taken, with its id.
    pub(crate) fn take_notices(&mut self) -> Vec<(u64, Notice)> {
        let mut out = Vec::new();
        for (id, t) in &mut self.made {
            out.extend(t.body.take_requests().into_iter().map(|n| (*id, n)));
        }
        out
    }

    /// The physics body of object `id`.
    pub(crate) fn body_mut(&mut self, id: u64) -> Option<&mut Body> {
        self.made.get_mut(&id).map(|t| &mut t.body)
    }

    /// The ids made, and what holds each.
    pub(crate) fn ids(&self) -> Vec<(u64, Option<(u64, u32)>)> {
        self.made.iter().map(|(id, t)| (*id, t.holder)).collect()
    }

    /// Turn on (`on`) or off the streaks of the objects made that `holder` holds at holding
    /// location `location` (all it holds for 0); the ids turned, for what they hold in turn.
    // D2-06197, D2-06198
    pub(crate) fn turn_streaks(&mut self, holder: u64, location: u32, on: bool) -> Vec<u64> {
        let mut turned = Vec::new();
        for (id, t) in &mut self.made {
            if !t
                .holder
                .is_some_and(|(p, at)| p == holder && (location == 0 || at == location))
            {
                continue;
            }
            if let Some(fx) = t.effects.as_mut() {
                fx.fx.turn_streaks(on);
            }
            turned.push(*id);
        }
        turned
    }

    /// Object `id` made from record `data_id` standing at `at` (not in the world), its timeout
    /// `seconds` from `now` (none when not above 0); false when the record makes no object.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create(
        &mut self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        id: u64,
        data_id: u32,
        seconds: f64,
        now: f64,
        at: Position,
    ) -> bool {
        let Some(look) = look_of_record(records, data_id) else {
            return false;
        };
        let figure = Figure::from_look(anims, &look);
        let mut body = body_of(id, figure.as_ref(), look.scale.z.max(0.01), at);
        body.look_scale = crate::pv(look.scale);
        body.child_scale = look.child_scale;
        self.made.insert(
            id,
            Temporary {
                data_id,
                maker: 0,
                dress: None,
                placed_at: None,
                figure,
                stood: false,
                body,
                scale: look.scale,
                holder: None,
                expires: (seconds > 0.0).then_some(now + seconds),
                removal: None,
                effects: crate::fx::BodyFx::of(&look, id),
            },
        );
        true
    }

    /// Object `id` takes the look of `visual`.
    pub(crate) fn set_look(
        &mut self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        id: u64,
        visual: &WireVisual,
    ) {
        let Some(t) = self.made.get_mut(&id) else {
            return;
        };
        let Some(look) = crate::figures::look_after(records, anims, visual, &mut t.dress) else {
            return;
        };
        crate::takes_look(anims, &mut t.figure, &mut t.effects, Some(&look), id);
        t.scale = look.scale;
        t.body.look_scale = crate::pv(look.scale);
        t.body.child_scale = look.child_scale;
    }

    /// Object `id` takes `values` of appearance table `apr` over its look.
    // D2-06907
    pub(crate) fn add_appearance(
        &mut self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        id: u64,
        apr: u32,
        values: &[(u32, f32)],
    ) {
        let Some(t) = self.made.get_mut(&id) else {
            return;
        };
        let mut parent = DataId(t.data_id);
        if parent.0 >> 24 == 0x47 {
            parent = records
                .entity(parent)
                .and_then(|e| e.object)
                .map_or(DataId::NONE, DataId);
        }
        let dress = t.dress.get_or_insert_with(|| {
            ObjectLook::new(&Desc {
                parent,
                ..Desc::default()
            })
        });
        for &(key, scalar) in values {
            let value = AppearanceKey { key, scalar };
            let _ = dress
                .local
                .add_appearance_key(anims, CLONE_APPEARANCE_GROUP, apr, value);
        }
        let Ok(look) = dress.total(anims) else {
            return;
        };
        crate::takes_look(anims, &mut t.figure, &mut t.effects, Some(&look), id);
    }

    /// Removes object `id` at once.
    pub(crate) fn remove(&mut self, id: u64) -> Option<Temporary> {
        self.made.remove(&id)
    }

    /// Fades object `id` out over `fade` seconds and removes it `delay` seconds from `now`: with no
    /// delay it goes at once, before the fade shows.
    // D2-05707
    pub(crate) fn fade_and_remove(&mut self, id: u64, now: f64, fade: f64, delay: f64) {
        let Some(t) = self.made.get_mut(&id) else {
            return;
        };
        if let Some(f) = t.figure.as_mut() {
            #[allow(clippy::cast_possible_truncation)]
            f.sliders.do_slider(0, FADE_SLIDER, 0.0, fade as f32);
        }
        let at = now + delay;
        t.removal = Some(t.removal.map_or(at, |r| r.min(at)));
    }

    /// What every object's figure asked beyond its animation since last taken, with its id. A
    /// behavior that destroys its object as it ends hides it at once, and it goes three seconds on.
    // D2-06097
    pub(crate) fn take_figure_events(
        &mut self,
        now: f64,
    ) -> Vec<(u64, auberean_animation::host::Event)> {
        use auberean_animation::host::Event;
        let mut out = Vec::new();
        let mut removed = Vec::new();
        for (id, t) in &mut self.made {
            if let Some(f) = &mut t.figure {
                for e in f.take_events() {
                    if e == Event::Remove {
                        removed.push(*id);
                    } else {
                        out.push((*id, e));
                    }
                }
            }
        }
        for id in removed {
            self.fade_and_remove(id, now, 0.0, REMOVED_GONE);
        }
        out
    }

    /// The objects whose timeout has passed by `now` fade out; those whose time to go has come are
    /// removed (their ids given back, with their holders).
    pub(crate) fn use_time(&mut self, now: f64) -> Vec<(u64, Option<u64>)> {
        let due: Vec<u64> = self
            .made
            .iter()
            .filter(|(_, t)| t.removal.is_none() && t.expires.is_some_and(|e| e <= now))
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            tracing::debug!(
                id = format_args!("{id:#x}"),
                in_world = self.made.get(&id).is_some_and(|t| t.body.in_world),
                held = self.made.get(&id).is_some_and(|t| t.holder.is_some()),
                "an object the client made fades out at its timeout"
            );
            self.fade_and_remove(id, now, FADE_TIME, FADE_TIME);
        }
        let gone: Vec<(u64, Option<u64>)> = self
            .made
            .iter()
            .filter(|(_, t)| t.removal.is_some_and(|r| r <= now))
            .map(|(id, t)| (*id, t.holder.map(|(h, _)| h)))
            .collect();
        for (id, _) in &gone {
            self.made.remove(id);
        }
        gone
    }

    /// The objects as batches in block `block`'s frame, each at its place in `placed`, noted as
    /// the other bodies are.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        drawer: &mut BodyDrawer,
        block: (i32, i32),
        placed: &[(u64, Position, dereth_primitives::Vec3)],
        out: &mut Vec<ObjectBatch>,
        noted: &mut Vec<(u64, std::ops::Range<usize>, Vec3, f32)>,
    ) {
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy) = (block.0 as f32 * BLOCK_LENGTH, block.1 as f32 * BLOCK_LENGTH);
        for (id, at, scale) in placed {
            let Some(f) = self
                .made
                .get(id)
                .filter(|t| !t.body.is_no_draw())
                .and_then(|t| t.figure.as_ref())
            else {
                continue;
            };
            let o = at.frame.origin;
            let g = crate::figures::global(at.cell.0, Vec3::new(o.x, o.y, o.z));
            let rel = Vec3::new(g.x - ox, g.y - oy, g.z);
            let q = at.frame.rotation;
            // D2-05981
            let place = Mat4::from_parts(
                rel,
                Quat {
                    w: q.w,
                    x: q.x,
                    y: q.y,
                    z: q.z,
                },
                Vec3::new(scale.x, scale.y, scale.z),
            );
            let start = out.len();
            if crate::figures::is_visible(f) {
                let poses = f.pose(anims);
                drawer.next_body(false);
                out.extend(drawer.batches(records, f, &poses, &place));
            }
            let heading = auberean_physics::geom::frame_heading(&at.frame).degrees();
            noted.push((*id, start..out.len(), rel, heading));
        }
    }
}

impl Scene<'_> {
    /// An object made from record `data_id` with a timeout of `seconds` (see [`Temporaries`]),
    /// under the lowest temporary id free, standing where the player does until it is placed:
    /// its id.
    // D2-05722
    pub(crate) fn create_temporary(
        &mut self,
        client: &mut Client,
        data_id: u32,
        seconds: f64,
    ) -> Option<u64> {
        let at = self.player.as_ref()?.body.position;
        let id = client.model.allocate_temporary_id()?;
        let now = client.local_time();
        if !self.temporaries.create(
            &self.world.records,
            &self.anims,
            id,
            data_id,
            seconds,
            now,
            at,
        ) {
            client.model.deallocate_temporary_id(id);
            return None;
        }
        // In the world's table with a new mover's collision settings: no game object sets others.
        if let Some(t) = self.temporaries.get_mut(id) {
            self.table.insert_body(&self.land, id, &mut t.body);
        }
        // Among the client's objects, with its timeout as its lifetime.
        client.queue.initialize_temporary_object(id, seconds, now);
        tracing::debug!(
            id = format_args!("{id:#x}"),
            record = format_args!("{data_id:#x}"),
            "an object the client made"
        );
        Some(id)
    }

    /// Removes an object the client made, at once, giving its id back.
    // D2-05706
    pub(crate) fn destroy_temporary(&mut self, client: &mut Client, id: u64) {
        if let Some(t) = self.temporaries.remove(id) {
            if let Some(k) = t.body.key() {
                self.table.remove(k);
            }
            self.holder_lets_go(t.holder.map(|(h, _)| h), id);
            self.stories.remove(&id);
            client.model.deallocate_temporary_id(id);
            client.queue.remove_object(id);
        }
    }

    /// A holder takes an object the client made off its list as the object goes, so that the id,
    /// when used again, is not taken for it.
    pub(crate) fn holder_lets_go(&mut self, holder: Option<u64>, id: u64) {
        let Some(holder) = holder else {
            return;
        };
        let body = match self.player.as_mut() {
            Some(p) if p.id == holder => Some(&mut p.body),
            _ => self.others.holder_mut(holder).map(|(b, _)| b),
        };
        if let Some(b) = body {
            b.remove_child(id);
        }
    }

    /// The record object `id` was made from: an object the client made, or the visual description
    /// the server gave an object.
    // D2-05839
    pub(crate) fn data_id_of(&self, client: &Client, id: u64) -> u32 {
        if let Some(t) = self.temporaries.get(id) {
            return t.data_id;
        }
        client.model.object(id).map_or(0, |o| {
            if o.visual.bits & WireVisual::DID != 0 {
                o.visual.did
            } else {
                0
            }
        })
    }

    /// Object `to` (one the client made) takes the look the server gave object `from`.
    // D2-06185, D2-06236
    pub(crate) fn copy_look(&mut self, client: &Client, from: u64, to: u64) {
        let Some(o) = client.model.object(from) else {
            return;
        };
        let visual = o.visual.clone();
        self.temporaries
            .set_look(&self.world.records, &self.anims, to, &visual);
    }

    /// Object `id`'s body takes collision settings `c` (the parts they mark set).
    // D2-05925
    pub(crate) fn set_body_collision_state(&mut self, id: u64, c: &CollisionSettings) {
        if let Some(k) = self.table.find(id) {
            self.table.set_collision_state(k, c);
        }
    }

    /// Object `id` (one the client made) is set up as a missile: its flight and its aim.
    // D2-06240
    pub(crate) fn missile_initialize(
        &mut self,
        id: u64,
        p: &MissileParameters,
        t: &MissileTargetingParameters,
    ) {
        let Some(m) = self.temporaries.get_mut(id) else {
            return;
        };
        m.body
            .missile_initialize(&self.land, &mut self.table, &mut self.objects, p, t);
    }

    /// Object `id` (one an effect made) hangs from object `holder` at holding location `location`
    /// (none: at the bone of kind `bone`), `offset` from there: it is no longer to be destroyed.
    // D2-05997
    pub(crate) fn fx_hold_temporary(
        &mut self,
        client: &mut Client,
        id: u64,
        holder: u64,
        (location, bone): (u32, u32),
        offset: dereth_primitives::Vec3,
        remove_rotation: bool,
    ) {
        let Some(t) = self.temporaries.get_mut(id) else {
            return;
        };
        let holder_body = match self.player.as_mut() {
            Some(p) if p.id == holder => &mut p.body,
            _ => match self.others.holder_mut(holder) {
                Some((b, _)) => b,
                None => return,
            },
        };
        let taken = auberean_physics::body::parent::fx_set_parent(
            &mut t.body,
            holder_body,
            location,
            bone,
            offset,
            remove_rotation,
            &self.land,
            &mut self.table,
            &mut self.objects,
        );
        if taken {
            t.holder = Some((holder, if location != 0 { location } else { bone }));
            client.queue.remove_object_to_be_destroyed(id);
        }
    }

    /// Object `id` (one the client made) is taken up by `holder`'s body at holding location
    /// `location`.
    // D2-06146
    pub(crate) fn hold_temporary(&mut self, id: u64, holder: u64, location: u32) {
        let Some(t) = self.temporaries.get_mut(id) else {
            return;
        };
        t.stood |= t.body.in_world;
        let (holder_body, holder_figure) = match self.player.as_mut() {
            Some(p) if p.id == holder => (&mut p.body, &p.figure),
            _ => match self.others.holder_mut(holder) {
                Some(h) => h,
                None => return,
            },
        };
        let how = TakeUp {
            location,
            holds: auberean_physics::body::parent::holds_at(
                location,
                holder_figure.has_holding_location(location),
            ),
            parent_mode: holder_figure.mode,
            child_is_temporary: true,
        };
        let taken = set_parent(
            &mut t.body,
            holder_body,
            how,
            &self.land,
            &mut self.table,
            &mut self.objects,
        );
        if taken {
            t.holder = Some((holder, location));
        }
        tracing::debug!(
            id = format_args!("{id:#x}"),
            holder = format_args!("{holder:#x}"),
            location = format_args!("{location:#x}"),
            holds = how.holds,
            taken,
            "an object the client made held"
        );
    }

    /// Object `id` (one the client made) is placed and fired as a missile at its next tick.
    // D2-06205
    pub(crate) fn fire_temporary(&mut self, id: u64) {
        if let Some(t) = self.temporaries.get_mut(id) {
            t.body.missile_place_and_fire();
        }
    }

    /// The objects the client made, over the frame: those whose time has come go; one held stands
    /// where it hangs from its holder; each runs the frame's `ticks` physics ticks (a missile's
    /// guidance places, launches and steers it), and what it asks of itself is played (fading out
    /// once it struck something, its flight effect); their figures animate over `dt` seconds, what
    /// they finished and the hooks they fired added to `heard`.
    pub(crate) fn update_temporaries(
        &mut self,
        client: &mut Client,
        ticks: u32,
        anim: Option<f32>,
        heard: &mut Vec<(u64, Vec<Completion>, Vec<u32>)>,
    ) {
        let now = client.local_time();
        for (id, holder) in self.temporaries.use_time(now) {
            if let Some(k) = self.table.find(id) {
                self.table.remove(k);
            }
            self.holder_lets_go(holder, id);
            self.stories.remove(&id);
            client.model.deallocate_temporary_id(id);
            client.queue.remove_object(id);
        }
        // Those the client let go of (asked to be destroyed, their time come) go with them.
        let gone: Vec<u64> = self
            .temporaries
            .ids()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| !client.queue.has_object(*id))
            .collect();
        for id in gone {
            self.destroy_temporary(client, id);
        }
        // Past the player's range, an object the client made goes, as its movement finds it there.
        // D2-06102
        if let Some(me) = self.player.as_ref().map(|p| p.body.position) {
            let far: Vec<u64> = self
                .temporaries
                .ids()
                .into_iter()
                .map(|(id, _)| id)
                .filter(|id| {
                    self.temporaries.get(*id).is_some_and(|t| {
                        t.body.movement_checks_range()
                            && !t.body.has_parent
                            && crate::others::out_of_physics_range(&me, &t.body.position)
                    })
                })
                .collect();
            for id in far {
                self.destroy_temporary(client, id);
            }
        }
        let game_now = client.game_time();
        for (id, holder) in self.temporaries.ids() {
            // Held, it stands where it hangs from its holder; let go of, it is held no more.
            if let Some((h, _)) = holder {
                let held = self.temporaries.get(id).is_some_and(|t| t.body.has_parent);
                let hanging = if held {
                    self.hanging_place(id, h)
                } else {
                    None
                };
                let Some(t) = self.temporaries.get_mut(id) else {
                    continue;
                };
                if let Some(at) = hanging {
                    t.body.position = at;
                }
                if !held {
                    t.holder = None;
                }
            }
            let Some(t) = self.temporaries.get_mut(id) else {
                continue;
            };
            let was_in_world = t.body.in_world;
            t.stood |= was_in_world;
            let was_active = t.body.missile.as_deref().is_some_and(|g| g.activated);
            // Held, it is put in the world where it hangs now.
            let hung_at = t.body.has_parent.then_some(t.body.position);
            for _ in 0..ticks {
                t.body.clock = game_now - TICK;
                #[allow(clippy::cast_possible_truncation)]
                let step = TICK as f32;
                t.body.update_in(
                    &self.land,
                    &mut self.table,
                    &mut self.objects,
                    &NoGameObjects,
                    step,
                );
            }
            if was_active && !t.body.missile.as_deref().is_some_and(|g| g.activated) {
                let target = t.body.missile.as_deref().map_or(0, |g| g.targeting.target);
                tracing::debug!(
                    id = format_args!("{id:#x}"),
                    at = ?t.body.position,
                    struck = format_args!("{:#x}", t.body.struck.unwrap_or(0)),
                    target = format_args!("{target:#x}"),
                    "an object the client made struck something"
                );
                if target != 0 && t.body.struck == Some(target) {
                    self.combat.missiles_hit_target += 1;
                    if self.player.as_ref().is_some_and(|p| p.id == t.maker) {
                        self.combat.players_hit_target += 1;
                    }
                }
            }
            if t.body.in_world && !was_in_world {
                tracing::debug!(
                    id = format_args!("{id:#x}"),
                    at = ?t.body.position,
                    "an object the client made put in the world"
                );
                self.combat.missiles_flying += 1;
                if let Some(p) = self.player.as_ref().filter(|p| p.id == t.maker) {
                    self.combat.players_flying += 1;
                    // Where the player let it go, in the player's own frame.
                    if let Some(at) = hung_at {
                        let d = auberean_physics::geom::offset(&p.body.position, &at);
                        let local =
                            auberean_physics::geom::global_to_local(&p.body.position.frame, d);
                        if local.y > 0.0 && local.z > 1.0 {
                            self.combat.players_let_go_ahead += 1;
                        }
                    }
                }
                t.placed_at = Some(t.body.position);
            }
            if let Some(from) = t.placed_at {
                let d = auberean_physics::geom::offset(&from, &t.body.position);
                if d.x * d.x + d.y * d.y + d.z * d.z > 1.0 {
                    t.placed_at = None;
                    self.combat.missiles_travelled += 1;
                }
            }
            if let (Some(f), Some(step)) = (t.figure.as_mut(), anim) {
                f.advance(&self.anims, step);
                heard.push((id, f.take_completions(), crate::take_triggers(f).0));
            }
        }
    }

    /// What object `id` (one the client made) asked of itself: fading out (removed once faded),
    /// its flight effect played or stopped, the rest for its figure. The notice is handed back for
    /// an object the client did not make.
    pub(crate) fn temporary_notice_or(&mut self, id: u64, n: Notice, now: f64) -> Option<Notice> {
        if !self.temporaries.has(id) {
            return Some(n);
        }
        match n {
            Notice::FadeOut { fade, delay } => {
                tracing::debug!(
                    id = format_args!("{id:#x}"),
                    "an object the client made fades out"
                );
                self.temporaries.fade_and_remove(id, now, fade, delay);
            }
            Notice::Animate(auberean_physics::body::Animate::Fx { fx, intensity }) => {
                self.do_fx(id, fx, intensity);
            }
            Notice::StopFx(fx) => self.stop_fx(id, fx),
            Notice::StickTo(object) => self.stick_temporary(id, object),
            n => {
                let anims = &self.anims;
                let figure = self.temporaries.figure_mut(id);
                crate::notices::hand_on(id, n, figure, anims);
            }
        }
        None
    }

    /// Object `id` (one the client made: a missile that struck `object`) sticks in `object` where
    /// it stands, held by the bone of its figure nearest it, until it fades.
    // D2-06222
    fn stick_temporary(&mut self, id: u64, object: u64) {
        let Some(t) = self.temporaries.get_mut(id) else {
            return;
        };
        let holder = match self.player.as_mut() {
            Some(p) if p.id == object => &mut p.body,
            _ => match self.others.body_mut(object) {
                Some(b) => b,
                None => return,
            },
        };
        let stuck = sticky_set_parent(
            &mut t.body,
            holder,
            true,
            &self.land,
            &mut self.table,
            &mut self.objects,
        );
        if stuck {
            t.holder = Some((object, 0));
            self.combat.missiles_stuck += 1;
        }
        tracing::debug!(
            id = format_args!("{id:#x}"),
            object = format_args!("{object:#x}"),
            stuck,
            "an object the client made sticks in what it struck"
        );
    }

    /// Where object `id` hangs from holder `holder`'s body as it stands now, when it is taken up.
    fn hanging_place(&self, id: u64, holder: u64) -> Option<Position> {
        self.hanging(id, holder, None).map(|(at, _)| at)
    }

    /// Where object `id` hangs from `holder` and what the holder gives its scale: from where the
    /// holder is drawn `alpha` of a tick past its last, or from where it stands.
    // D2-06182
    fn hanging(
        &self,
        id: u64,
        holder: u64,
        alpha: Option<f32>,
    ) -> Option<(Position, (dereth_primitives::Vec3, f32))> {
        let (hb, hf) = match &self.player {
            Some(p) if p.id == holder => (&p.body, &p.figure),
            _ => {
                let (b, f, _) = self.others.holder(holder)?;
                (b, f)
            }
        };
        let info = hb
            .children
            .as_ref()
            .and_then(|c| c.children.iter().find(|c| c.id == id))?;
        let m = crate::held::hanging(hf, &self.anims, info);
        let whole = hb.total_scale(None);
        let at = alpha.map_or(hb.position, |a| hb.drawn(a).0);
        Some((
            Body::child_place(&at, whole, info, m.as_ref()),
            (whole, hb.child_scale),
        ))
    }

    /// How much of an object the client made shows, 0 to 1, when it has a figure.
    #[must_use]
    pub fn temporary_opacity(&self, id: u64) -> Option<f32> {
        let f = self.temporaries.get(id)?.figure.as_ref()?;
        Some(f.sliders.current(FADE_SLIDER).clamp(0.0, 1.0))
    }

    /// The objects the client made as they are drawn this frame `alpha` of a tick past their last:
    /// each with where it stands and its scale. One held by the player is left out when the player
    /// is not drawn.
    // D2-06149, D2-06182
    #[must_use]
    pub fn temporaries_placed(
        &self,
        alpha: f32,
        player_drawn: bool,
    ) -> Vec<(u64, Position, dereth_primitives::Vec3)> {
        let mut out = Vec::new();
        for (id, holder) in self.temporaries.ids() {
            let Some(t) = self.temporaries.get(id) else {
                continue;
            };
            let place = match holder {
                Some((h, _)) => {
                    if !player_drawn && self.player.as_ref().is_some_and(|p| p.id == h) {
                        continue;
                    }
                    self.hanging(id, h, Some(alpha))
                        .map(|(at, given)| (at, t.body.total_scale(Some(given))))
                }
                None if t.body.in_world => Some((t.body.drawn(alpha).0, t.body.total_scale(None))),
                None => None,
            };
            if let Some((at, scale)) = place {
                out.push((id, at, scale));
            }
        }
        out
    }
}
