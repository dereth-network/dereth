//! A body's collisions: the objects it touches, their properties and pushes, the water and ground
//! it is in or on, and what it reports of them each step.

use std::collections::BTreeMap;

use auberean_assets::property::{PhysicsMaterial, PropertyGroup, PropertyList};
use auberean_assets::Context;
use auberean_dat::reader::Result;
use auberean_dat::Reader;
use dereth_primitives::shape::Plane;
use dereth_primitives::{Position, Vec3};

use super::{
    same_property, CollisionReport, CollisionSettings, ContactProperties, ContactProperty,
    ContactRecord, ObjectContact, Push,
};
use crate::ethereal::PhysicsTypes;
use crate::geom;
use crate::pack::{
    read_packs, read_plane, read_vec3, write_packs, write_plane, write_table_header, write_vec3,
    Pack, Writer,
};

/// The property naming a surface's terrain type.
pub const TERRAIN: u32 = 0x4000_0036;
/// No terrain type.
pub const NO_TERRAIN: u32 = 0x8000_0001;

/// The push properties an object's properties may carry (vectors), and the flag saying the
/// vectors are in the object's own frame.
pub mod push {
    pub const WALK_ON_AIR_VELOCITY: u32 = 0x4000_0040;
    pub const WALK_ON_AIR_ACCELERATION: u32 = 0x4000_0041;
    pub const VELOCITY: u32 = 0x4000_0042;
    pub const ACCELERATION: u32 = 0x4000_0043;
    pub const EARTH_VELOCITY: u32 = 0x4000_0044;
    pub const EARTH_ACCELERATION: u32 = 0x4000_0045;
    pub const AIR_VELOCITY: u32 = 0x4000_0046;
    pub const AIR_ACCELERATION: u32 = 0x4000_0047;
    pub const SEAFLOOR_VELOCITY: u32 = 0x4000_0048;
    pub const SEAFLOOR_ACCELERATION: u32 = 0x4000_0049;
    pub const WATER_VELOCITY: u32 = 0x4000_004A;
    pub const WATER_ACCELERATION: u32 = 0x4000_004B;
    pub const LOCAL: u32 = 0x4000_004C;
}

/// Where a body is, for the pushes that depend on it.
pub mod element {
    pub const EARTH: u32 = 0x4000_0001;
    pub const AIR: u32 = 0x4000_0002;
    pub const WATER: u32 = 0x4000_0003;
    pub const SEAFLOOR: u32 = 0x4000_0004;
}

/// The handle a body keeps another body under (scenery has no object id, so the handle, not the
/// id, tells them apart).
pub type BodyKey = u32;

/// The world as a body's collisions see it: the body itself, the bodies it touches, the property
/// list and the physics materials.
pub trait CollisionHost {
    /// This body.
    fn key(&self) -> BodyKey;
    /// A body's object id; 0 for none.
    fn object_id(&self, body: BodyKey) -> u64;
    /// The body with an object id, when it is in the world.
    fn find(&self, object: u64) -> Option<BodyKey>;
    /// Whether `body` and `other` pass through each other, this body's ([`CollisionHost::key`])
    /// collision settings being `own`, the caller's.
    fn interpermeable(&self, body: BodyKey, other: BodyKey, own: &CollisionSettings) -> bool;
    /// What a body says of itself when touched; `None` when it is not to be recorded.
    fn contact_record(&self, body: BodyKey) -> Option<ContactRecord>;
    fn ignores_collisions(&self, body: BodyKey) -> bool;
    /// The physics types a body is.
    fn collide_mask(&self, body: BodyKey) -> PhysicsTypes;
    /// A body's own properties, when it has any.
    fn object_properties(&self, body: BodyKey) -> Option<Vec<PropertyGroup>>;
    fn position(&self, body: BodyKey) -> Position;
    /// Tells `body` to pass through `object` for now.
    fn add_to_temp_ethereal_list(&mut self, body: BodyKey, object: u64);
    /// Tells `body` that `object` no longer touches it.
    fn collision_remove_object(&mut self, body: BodyKey, object: u64);
    /// Sets this body's physics type.
    fn set_physics_collide_mask(&mut self, mask: &PhysicsTypes);
    /// Tells this body the terrain type it is on.
    fn set_terrain_type(&mut self, terrain: u32);
    /// Whether this body reports its collisions.
    fn reports_collisions(&self) -> bool;
    fn physics_material(&self, id: u32) -> Option<PhysicsMaterial>;
    /// The install's property list.
    fn property_list(&self) -> &PropertyList;
    /// The clock, seconds.
    fn now(&self) -> f64;
}

/// One body's collisions.
// D2-06780, D2-06760, D2-06761, D2-06762
#[derive(Clone, Debug, PartialEq)]
pub struct Collisions {
    /// The physics types the body would pass through moving.
    pub desired_physics: PhysicsTypes,
    pub current: CollisionSettings,
    /// The physics types of the scenery the body has touched and passes through.
    pub collide_mask: PhysicsTypes,
    pub contacts: BTreeMap<u64, ObjectContact>,
    pub property_lists: BTreeMap<BodyKey, ContactProperties>,
    /// The number the next contact property gets.
    pub next_property_id: u32,
    pub has_normal: bool,
    pub normal: Vec3,
    pub impact_speed: f32,
    pub has_contact: bool,
    pub contact_plane: Plane,
    pub in_water: bool,
    pub water_material: u32,
    pub on_landscape: bool,
    pub landscape_material: u32,
    pub reported_has_contact: bool,
    pub reported_contact_plane: Plane,
    pub reported_in_water: bool,
    pub reported_water_material: u32,
    pub reported_on_landscape: bool,
    pub reported_landscape_material: u32,
    pub env_properties_updated: bool,
    /// The time of the step's collision check; touches stamped with another time are old.
    pub last_check_time: f64,
    pub water_properties: Vec<ContactProperty>,
    pub surface_properties: Vec<ContactProperty>,
    pub env_properties: Vec<ContactProperty>,
    /// Whether every touch has been settled.
    pub resolved: bool,
    pub water_terrain: u32,
    pub land_terrain: u32,
    pub prev_terrain: u32,
    pub pushers: BTreeMap<BodyKey, Push>,
}

impl Default for Collisions {
    // D2-06792
    fn default() -> Self {
        Self {
            desired_physics: PhysicsTypes::default(),
            current: CollisionSettings::default(),
            collide_mask: PhysicsTypes::new(u64::MAX),
            contacts: BTreeMap::new(),
            property_lists: BTreeMap::new(),
            next_property_id: 0,
            has_normal: false,
            normal: Vec3::ZERO,
            impact_speed: 0.0,
            has_contact: false,
            contact_plane: Plane::default(),
            in_water: false,
            water_material: 0,
            on_landscape: false,
            landscape_material: 0,
            reported_has_contact: false,
            reported_contact_plane: Plane::default(),
            reported_in_water: false,
            reported_water_material: 0,
            reported_on_landscape: false,
            reported_landscape_material: 0,
            env_properties_updated: false,
            last_check_time: 0.0,
            water_properties: Vec::new(),
            surface_properties: Vec::new(),
            env_properties: Vec::new(),
            resolved: true,
            water_terrain: NO_TERRAIN,
            land_terrain: NO_TERRAIN,
            prev_terrain: NO_TERRAIN,
            pushers: BTreeMap::new(),
        }
    }
}

/// Removes the ended properties, the last taking each one's place; whether any was removed.
// D2-06733
fn remove_end_elements(list: &mut Vec<ContactProperty>) -> bool {
    let mut removed = false;
    let mut i = 0;
    while i < list.len() {
        if list[i].is_end() {
            list.swap_remove(i);
            removed = true;
        } else {
            i += 1;
        }
    }
    removed
}

/// Makes `list` the inquirable properties of `groups`, numbered from `next_id`.
// D2-06734
fn set_properties(
    groups: &[PropertyGroup],
    list: &mut ContactProperties,
    next_id: &mut u32,
    props: &PropertyList,
) {
    list.properties.clear();
    for p in groups.iter().flat_map(|g| g.properties.iter()) {
        if props.is_inquirable(p.id) {
            list.add(ContactProperty::new(p.clone(), *next_id));
            *next_id = next_id.wrapping_add(1);
        }
    }
}

/// Brings `list` up to the inquirable properties of `groups`: one already held (among those held
/// before) becomes unchanged or changed, a new one begins. Whether anything changed or began.
// D2-06757
fn update_property_array(
    groups: &[PropertyGroup],
    list: &mut Vec<ContactProperty>,
    next_id: &mut u32,
    props: &PropertyList,
) -> bool {
    let held = list.len();
    let mut changed = false;
    for p in groups.iter().flat_map(|g| g.properties.iter()) {
        if !props.is_inquirable(p.id) {
            continue;
        }
        if let Some(c) = list[..held].iter_mut().find(|c| c.name() == p.id) {
            if same_property(p, &c.property) {
                c.set_as_same();
            } else {
                c.update_value(p);
                c.set_as_change();
                changed = true;
            }
            continue;
        }
        let mut c = ContactProperty::new(p.clone(), *next_id);
        *next_id = next_id.wrapping_add(1);
        c.set_as_begin();
        list.push(c);
        changed = true;
    }
    changed
}

fn add(a: &mut Vec3, b: Vec3) {
    a.x += b.x;
    a.y += b.y;
    a.z += b.z;
}

impl Collisions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // D2-06717
    pub fn set_min_reported_impact_z(&mut self, v: f32) {
        self.current.min_impact_z = v;
    }

    // D2-06718
    pub fn set_min_reported_impact_xy(&mut self, v: f32) {
        self.current.min_impact_xy = v;
    }

    /// What the body passes through being placed, and moving.
    // D2-06719
    pub fn collision_state(&self, out: &mut CollisionSettings) {
        out.placement_physics = self.current.placement_physics;
        out.set_bit(super::settings::PLACEMENT_PHYSICS);
        out.movement_physics = self.desired_physics;
        out.set_bit(super::settings::MOVEMENT_PHYSICS);
    }

    /// Settles whether the body passes through a touched object: if it does now, the touch is
    /// ethereal; if it did and no longer does, both pass through each other for now.
    // D2-06720
    pub fn has_conflict(&mut self, c: &mut ObjectContact, host: &mut dyn CollisionHost) {
        let id = c.id();
        if id == 0 {
            return;
        }
        let Some(other) = host.find(id) else { return };
        if host.interpermeable(host.key(), other, &self.current) {
            c.record.ethereal = true;
            return;
        }
        if c.record.ethereal {
            c.record.ethereal = false;
            self.current.add_to_temp_ethereal_list(id);
            let me = host.object_id(host.key());
            host.add_to_temp_ethereal_list(other, me);
        }
    }

    // D2-06721
    pub fn add_to_temp_ethereal_list(&mut self, object: u64) {
        self.current.add_to_temp_ethereal_list(object);
    }

    /// Takes on a lowest new physics type of scenery touched, unless the body already passes
    /// through one of its types.
    // D2-06722
    pub fn update_collide_physics_mask(&mut self, t: &PhysicsTypes) {
        if self.collide_mask.mask & t.mask == 0 {
            self.collide_mask.update_ethereality(t);
        }
    }

    /// Keeps a collision normal when the hit's vertical speed exceeds the least reported (at
    /// least 0.0002); its impact speed is the speed into the normal's vertical part.
    // D2-06723
    pub fn set_collision_normal(&mut self, normal: Vec3, velocity: Vec3) {
        let mut min = self.current.min_impact_z;
        if min < geom::EPS {
            min = geom::EPS;
        }
        if min < velocity.z.abs() {
            self.normal = normal;
            // Retail folds the normal's x and y into a product with zero.
            self.impact_speed = (normal.x + normal.y) * 0.0 - velocity.z * normal.z;
            self.has_normal = true;
        }
    }

    // D2-06724
    pub fn set_no_collision_normal(&mut self) {
        self.has_normal = false;
    }

    // D2-06725
    pub fn set_contact_plane(&mut self, p: Plane) {
        self.contact_plane = p;
        self.has_contact = true;
    }

    // D2-06726
    pub fn set_in_water(&mut self, material: u32) {
        self.in_water = true;
        self.water_material = material;
    }

    // D2-06727
    pub fn set_out_of_water(&mut self) {
        self.in_water = false;
    }

    // D2-06728
    pub fn set_on_landscape(&mut self, material: u32) {
        self.on_landscape = true;
        self.landscape_material = material;
    }

    // D2-06729
    pub fn set_off_landscape(&mut self) {
        self.on_landscape = false;
        self.landscape_material = 0;
    }

    // D2-06730
    pub fn set_terrain_type(&mut self, terrain: u32, host: &mut dyn CollisionHost) {
        if terrain != self.prev_terrain {
            host.set_terrain_type(terrain);
            self.prev_terrain = terrain;
        }
    }

    /// The water's terrain type from its terrain property: ended, none (the ground's then shows);
    /// otherwise the property's value. False when the property has no such value.
    // D2-06735
    pub fn update_water_terrain(
        &mut self,
        p: &ContactProperty,
        host: &mut dyn CollisionHost,
    ) -> bool {
        if p.is_end() {
            self.water_terrain = NO_TERRAIN;
            self.set_terrain_type(self.land_terrain, host);
            return true;
        }
        match p.enumeration() {
            Some(t) => {
                self.water_terrain = t;
                self.set_terrain_type(t, host);
                true
            }
            None => false,
        }
    }

    /// The ground's terrain type from its terrain property, shown when there is no water's.
    // D2-06736
    pub fn update_land_terrain(
        &mut self,
        p: &ContactProperty,
        host: &mut dyn CollisionHost,
    ) -> bool {
        let ok = if p.is_end() {
            self.land_terrain = NO_TERRAIN;
            true
        } else if let Some(t) = p.enumeration() {
            self.land_terrain = t;
            true
        } else {
            false
        };
        if self.water_terrain == NO_TERRAIN && ok {
            self.set_terrain_type(self.land_terrain, host);
        }
        ok
    }

    /// Whether the body touches nothing.
    // D2-06745
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.contacts.is_empty() && self.property_lists.is_empty()
    }

    /// The physics types the body passes through moving gain those it would.
    // D2-06750
    pub fn update_current_physics_mask(&mut self) {
        self.current.movement_physics.mask |= self.desired_physics.mask;
        self.current.set_bit(super::settings::MOVEMENT_PHYSICS);
    }

    /// The surroundings' properties that changed and are reported.
    // D2-06753
    pub fn report_environmental_properties(
        &self,
        report: &mut CollisionReport,
        props: &PropertyList,
    ) {
        for p in &self.env_properties {
            if !p.is_same() && p.is_reported(props) {
                report.env_properties.push(p.clone());
            }
        }
    }

    /// Every surroundings property ends and is reported, and the body has none.
    // D2-06754
    pub fn end_environmental_properties(&mut self, report: &mut CollisionReport) {
        for p in &mut self.env_properties {
            p.set_as_end();
            report.env_properties.push(p.clone());
        }
        self.env_properties.clear();
    }

    /// The water's properties that changed: its terrain type taken, the reported ones reported.
    // D2-06755
    pub fn report_water_properties(
        &mut self,
        report: &mut CollisionReport,
        host: &mut dyn CollisionHost,
    ) {
        let list = self.water_properties.clone();
        for p in list.iter().filter(|p| !p.is_same()) {
            if p.name() == TERRAIN {
                self.update_water_terrain(p, host);
            }
            if p.is_reported(host.property_list()) {
                report.water_properties.push(p.clone());
            }
        }
    }

    /// The ground's properties that changed: its terrain type taken, the reported ones reported.
    // D2-06756
    pub fn report_surface_properties(
        &mut self,
        report: &mut CollisionReport,
        host: &mut dyn CollisionHost,
    ) {
        let list = self.surface_properties.clone();
        for p in list.iter().filter(|p| !p.is_same()) {
            if p.name() == TERRAIN {
                self.update_land_terrain(p, host);
            }
            if p.is_reported(host.property_list()) {
                report.surface_properties.push(p.clone());
            }
        }
    }

    /// What the body's pushes add where it is (`element`): each push's velocity and acceleration,
    /// and those of the element. A push that walks on air first replaces what was summed with its
    /// own velocity and acceleration and then adds them again, as retail does.
    // D2-06758
    pub fn update_pusher_values(
        &self,
        element: u32,
        walk_on_air: &mut bool,
        velocity: &mut Vec3,
        acceleration: &mut Vec3,
    ) {
        *walk_on_air = false;
        for p in self.pushers.values() {
            if p.walk_on_air {
                *walk_on_air = true;
                *velocity = p.general_velocity;
                *acceleration = p.general_acceleration;
            }
            add(velocity, p.general_velocity);
            add(acceleration, p.general_acceleration);
            let (v, a) = match element {
                element::EARTH => (p.earth_velocity, p.earth_acceleration),
                element::AIR => (p.air_velocity, p.air_acceleration),
                element::WATER => (p.water_velocity, p.water_acceleration),
                element::SEAFLOOR => (p.seafloor_velocity, p.seafloor_acceleration),
                _ => continue,
            };
            add(velocity, v);
            add(acceleration, a);
        }
    }

    /// Settles every touched object's ethereality after the body's settings changed.
    // D2-06759
    pub fn update_conflict_list(&mut self, host: &mut dyn CollisionHost) {
        self.update_current_physics_mask();
        let keys: Vec<u64> = self.contacts.keys().copied().collect();
        for k in keys {
            if let Some(mut c) = self.contacts.get(&k).copied() {
                self.has_conflict(&mut c, host);
                self.contacts.insert(k, c);
            }
        }
    }

    /// Takes the parts of `s` it says are set, then settles every touched object.
    // D2-06764
    pub fn set_collision_state(&mut self, s: &CollisionSettings, host: &mut dyn CollisionHost) {
        use super::settings as b;
        if s.has(b::MOVEMENT_PHYSICS) {
            self.desired_physics = s.movement_physics;
            self.current.movement_physics = s.movement_physics;
            self.current.set_bit(b::MOVEMENT_PHYSICS);
        }
        if s.has(b::MOVEMENT_OBJECTS) {
            s.movement_objects
                .update(&mut self.current.movement_objects);
            self.current.set_bit(b::MOVEMENT_OBJECTS);
        }
        if s.has(b::PLACEMENT_PHYSICS) {
            self.current.placement_physics = s.placement_physics;
            self.current.set_bit(b::PLACEMENT_PHYSICS);
        }
        if s.has(b::PLACEMENT_OBJECTS) {
            s.placement_objects
                .update(&mut self.current.placement_objects);
            self.current.set_bit(b::PLACEMENT_OBJECTS);
        }
        if s.has(b::PHYSICS_TYPE) {
            host.set_physics_collide_mask(&s.physics_type);
        }
        if s.min_impact_xy != 0.0 {
            self.current.min_impact_xy = s.min_impact_xy;
        }
        if s.min_impact_z != 0.0 {
            self.current.min_impact_z = s.min_impact_z;
        }
        self.update_conflict_list(host);
    }

    /// The surroundings' properties: each held one ends unless the material still has it, the
    /// material's new ones begin, and the ended ones go.
    // D2-06765
    pub fn add_environmental_properties(
        &mut self,
        material: Option<&PhysicsMaterial>,
        props: &PropertyList,
    ) {
        for p in &mut self.env_properties {
            p.set_as_end();
        }
        if let Some(m) = material {
            if update_property_array(
                &m.properties.groups,
                &mut self.env_properties,
                &mut self.next_property_id,
                props,
            ) {
                self.env_properties_updated = true;
            }
        }
        if remove_end_elements(&mut self.env_properties) {
            self.env_properties_updated = true;
        }
    }

    /// The water's or the ground's properties for a new physics material: each held one ends
    /// unless the material has it too.
    // D2-06766
    pub fn change_physics_material(
        &mut self,
        material: u32,
        water: bool,
        host: &dyn CollisionHost,
    ) {
        let mut list = std::mem::take(if water {
            &mut self.water_properties
        } else {
            &mut self.surface_properties
        });
        for p in &mut list {
            p.set_as_end();
        }
        if material != 0 {
            if let Some(m) = host.physics_material(material) {
                update_property_array(
                    &m.properties.groups,
                    &mut list,
                    &mut self.next_property_id,
                    host.property_list(),
                );
            }
        }
        *(if water {
            &mut self.water_properties
        } else {
            &mut self.surface_properties
        }) = list;
    }

    /// Forgets a touched body's properties, and its push when it had properties.
    // D2-06767
    pub fn remove_properties_and_erase(&mut self, key: BodyKey) {
        if self.property_lists.contains_key(&key) {
            self.pushers.remove(&key);
        }
        self.property_lists.remove(&key);
    }

    /// An object leaves: its touch is marked to go, its reported properties end, and its properties
    /// and push are forgotten. Whether the report has anything.
    // D2-06768
    pub fn remove_object_collision(
        &mut self,
        object: u64,
        report: &mut CollisionReport,
        props: &PropertyList,
    ) -> bool {
        if let Some(c) = self.contacts.get_mut(&object) {
            c.remove_now = true;
        }
        let mut key = 0;
        if let Some(l) = self
            .property_lists
            .values_mut()
            .find(|l| l.object == object)
        {
            key = l.key;
            if l.reported {
                l.end_property_collision(report, props);
            }
        }
        self.remove_properties_and_erase(key);
        report.non_empty()
    }

    // D2-06781
    pub fn end_in_water(&mut self, report: &mut CollisionReport, host: &mut dyn CollisionHost) {
        for p in &mut self.water_properties {
            p.set_as_end();
        }
        self.report_water_properties(report, host);
        remove_end_elements(&mut self.water_properties);
    }

    // D2-06782
    pub fn end_on_landscape(&mut self, report: &mut CollisionReport, host: &mut dyn CollisionHost) {
        for p in &mut self.surface_properties {
            p.set_as_end();
        }
        self.report_surface_properties(report, host);
        remove_end_elements(&mut self.surface_properties);
    }

    /// The push a touched body's properties give, kept under its key unless one is kept already.
    /// Its vectors are turned from the body's frame when its local flag says so. Every push
    /// property makes a push except the water velocity alone, as retail does.
    // D2-06783
    pub fn add_pusher_properties(
        &mut self,
        body: BodyKey,
        list: &ContactProperties,
        host: &dyn CollisionHost,
    ) {
        let mut local = false;
        for p in &list.properties {
            if p.name() == push::LOCAL {
                if let Some(f) = p.flag() {
                    local = f;
                }
            }
        }
        let mut v = Push::default();
        let mut any = false;
        for p in &list.properties {
            let name = p.name();
            if !(push::WALK_ON_AIR_VELOCITY..=push::WATER_ACCELERATION).contains(&name) {
                continue;
            }
            let Some(mut x) = p.vector() else { continue };
            if !(geom::EPS <= x.x.abs() || geom::EPS <= x.y.abs() || geom::EPS <= x.z.abs()) {
                continue;
            }
            if local {
                x = geom::local_to_global(&host.position(body).frame, x);
            }
            if name != push::WATER_VELOCITY {
                any = true;
            }
            let to = match name {
                push::WALK_ON_AIR_VELOCITY => {
                    v.walk_on_air = true;
                    &mut v.general_velocity
                }
                push::WALK_ON_AIR_ACCELERATION => {
                    v.walk_on_air = true;
                    &mut v.general_acceleration
                }
                push::VELOCITY => &mut v.general_velocity,
                push::ACCELERATION => &mut v.general_acceleration,
                push::EARTH_VELOCITY => &mut v.earth_velocity,
                push::EARTH_ACCELERATION => &mut v.earth_acceleration,
                push::AIR_VELOCITY => &mut v.air_velocity,
                push::AIR_ACCELERATION => &mut v.air_acceleration,
                push::SEAFLOOR_VELOCITY => &mut v.seafloor_velocity,
                push::SEAFLOOR_ACCELERATION => &mut v.seafloor_acceleration,
                push::WATER_VELOCITY => &mut v.water_velocity,
                _ => &mut v.water_acceleration,
            };
            add(to, x);
        }
        if any {
            self.pushers.entry(body).or_insert(v);
        }
    }

    /// Starts keeping a touched object, stamped with this check's time.
    // D2-06784
    pub fn add_to_collide_list(&mut self, record: ContactRecord, ignore: bool) {
        let c = ObjectContact {
            touched_time: self.last_check_time,
            reported: false,
            remove_now: false,
            ignore,
            ignore_changed: false,
            record,
        };
        self.contacts.entry(record.object).or_insert(c);
    }

    /// Reports what began and what ended. A touch not renewed by this step's check and more than a
    /// second old ends and is forgotten; so does every touch when `leaving`, and when `end_all`
    /// every touch ends. A touch begins when the body reports collisions. Then, unless leaving, the
    /// collision normal, the water's and the ground's properties when the material changed, and
    /// the surroundings' when they changed; when leaving, all of those end.
    // D2-06787
    pub fn report_collision(
        &mut self,
        report: &mut CollisionReport,
        end_all: bool,
        leaving: bool,
        host: &mut dyn CollisionHost,
    ) {
        self.resolved = true;
        let now = host.now();
        let keys: Vec<u64> = self.contacts.keys().copied().collect();
        for k in keys {
            let Some(mut c) = self.contacts.get(&k).copied() else {
                continue;
            };
            let renewed = self.last_check_time == c.touched_time;
            let gap = now - c.touched_time;
            #[allow(clippy::cast_possible_truncation)]
            let dt = gap as f32;
            if (gap > 1.0 && !renewed) || end_all || c.remove_now || leaving {
                if c.reported {
                    if !c.ignore || c.ignore_changed {
                        report.add_as_end_collision(&mut c);
                    }
                    c.reported = false;
                }
                if (dt > 1.0 && !renewed) || c.remove_now || leaving {
                    let id = c.id();
                    self.contacts.remove(&id);
                    if id != 0 {
                        if let Some(other) = host.find(id) {
                            let me = host.object_id(host.key());
                            host.collision_remove_object(other, me);
                        }
                    }
                    self.current.remove_from_temp_ethereal_list(id);
                    continue;
                }
            } else if !c.reported && host.reports_collisions() {
                if !c.ignore {
                    report.add_as_begin_collision(&mut c);
                }
                c.reported = true;
            } else {
                if c.ignore_changed && c.reported {
                    if c.ignore {
                        report.add_as_end_collision(&mut c);
                    } else {
                        report.add_as_begin_collision(&mut c);
                    }
                    c.ignore_changed = false;
                }
                if dt > 0.0 {
                    self.resolved = false;
                }
            }
            self.contacts.insert(k, c);
        }

        let keys: Vec<BodyKey> = self.property_lists.keys().copied().collect();
        for k in keys {
            let Some(mut l) = self.property_lists.remove(&k) else {
                continue;
            };
            let renewed = self.last_check_time == l.touched_time;
            let gap = now - l.touched_time;
            #[allow(clippy::cast_possible_truncation)]
            let dt = gap as f32;
            let mut erase = false;
            if (gap > 1.0 && !renewed) || end_all || l.remove_now || leaving {
                if l.reported {
                    l.end_property_collision(report, host.property_list());
                    l.reported = false;
                }
                erase = (dt > 1.0 && !renewed) || l.remove_now || leaving;
            } else if !l.reported && host.reports_collisions() {
                l.begin_property_collision(report, host.property_list());
                l.reported = true;
            } else if dt > 0.0 {
                self.resolved = false;
            }
            self.property_lists.insert(k, l);
            if erase {
                self.remove_properties_and_erase(k);
            }
        }

        if host.reports_collisions() {
            if leaving {
                if self.reported_in_water {
                    self.end_in_water(report, host);
                    self.reported_in_water = false;
                }
                if self.reported_on_landscape {
                    self.end_on_landscape(report, host);
                    self.reported_on_landscape = false;
                }
                self.end_environmental_properties(report);
                self.env_properties_updated = false;
            } else {
                if self.has_normal {
                    report.normal_state = super::state::BEGIN;
                    report.normal = self.normal;
                    report.impact_speed = self.impact_speed;
                }
                if !self.in_water {
                    if self.reported_in_water {
                        self.end_in_water(report, host);
                    }
                } else if !self.reported_in_water
                    || self.water_material != self.reported_water_material
                {
                    self.change_physics_material(self.water_material, true, host);
                    self.report_water_properties(report, host);
                    remove_end_elements(&mut self.water_properties);
                    self.reported_water_material = self.water_material;
                }
                if !self.on_landscape {
                    if self.reported_on_landscape {
                        self.end_on_landscape(report, host);
                    }
                } else if !self.reported_on_landscape
                    || self.landscape_material != self.reported_landscape_material
                {
                    self.change_physics_material(self.landscape_material, false, host);
                    self.report_surface_properties(report, host);
                    remove_end_elements(&mut self.surface_properties);
                    self.reported_landscape_material = self.landscape_material;
                }
                if self.env_properties_updated {
                    self.report_environmental_properties(report, host.property_list());
                    self.env_properties_updated = false;
                }
                self.reported_has_contact = self.has_contact;
                self.reported_in_water = self.in_water;
                self.reported_on_landscape = self.on_landscape;
            }
        }
        self.normal = Vec3::ZERO;
        self.impact_speed = 0.0;
        self.has_normal = false;
    }

    // D2-06788
    pub fn add_to_property_list(&mut self, list: ContactProperties) {
        self.property_lists.entry(list.key).or_insert(list);
    }

    /// The body touches `other` in this step's check. Scenery with no object id adds its physics
    /// types to those the body passes through, when the body passes through it. An object is kept
    /// (or its keeping renewed and its ignoring updated). Then the object's properties are kept and
    /// its push taken, or their keeping renewed. False when the touch could not be kept.
    // D2-06793, D2-06769, D2-05928
    pub fn add_object_collision(&mut self, other: BodyKey, host: &mut dyn CollisionHost) -> bool {
        let id = host.object_id(other);
        if id == 0 {
            if host.interpermeable(host.key(), other, &self.current) {
                self.collide_mask.mask |= host.collide_mask(other).mask;
            }
        } else if let Some(c) = self.contacts.get_mut(&id) {
            let ignore = host.ignores_collisions(other);
            if ignore != c.ignore {
                c.update_ignore_collide(ignore);
            }
            c.touched_time = self.last_check_time;
        } else if let Some(mut record) = host.contact_record(other) {
            record.ethereal = host.interpermeable(other, host.key(), &self.current);
            let ignore = host.ignores_collisions(other);
            self.add_to_collide_list(record, ignore);
        }
        let Some(groups) = host.object_properties(other) else {
            return true;
        };
        if let Some(l) = self.property_lists.get_mut(&other) {
            l.touched_time = self.last_check_time;
            return true;
        }
        let mut list = ContactProperties {
            key: other,
            object: id,
            ..ContactProperties::default()
        };
        set_properties(
            &groups,
            &mut list,
            &mut self.next_property_id,
            host.property_list(),
        );
        list.touched_time = self.last_check_time;
        self.add_pusher_properties(other, &list, host);
        self.add_to_property_list(list);
        true
    }
}

fn read_table<K: Ord, V>(
    r: &mut Reader<'_>,
    mut entry: impl FnMut(&mut Reader<'_>) -> Result<(K, V)>,
) -> Result<BTreeMap<K, V>> {
    Ok(r.hash_table(4, |r| entry(r))?.into_iter().collect())
}

impl Pack for Collisions {
    /// The settings, the touched objects and properties, the normal, contact, water and ground and
    /// what was reported of them, the property lists, the terrain types and the pushes; the check
    /// time is not sent.
    // D2-06744, D2-06789, D2-06790, D2-06791
    fn pack(&self, w: &mut Writer) {
        self.desired_physics.pack(w);
        self.current.pack(w);
        self.collide_mask.pack(w);
        write_table_header(w, self.contacts.len());
        for (k, c) in &self.contacts {
            w.u64(*k);
            c.pack(w);
        }
        write_table_header(w, self.property_lists.len());
        for (k, l) in &self.property_lists {
            w.u32(*k);
            l.pack(w);
        }
        w.u32(self.next_property_id);
        w.bool32(self.has_normal);
        write_vec3(w, self.normal);
        w.f32(self.impact_speed);
        w.bool32(self.has_contact);
        write_plane(w, &self.contact_plane);
        w.bool32(self.in_water);
        w.u32(self.water_material);
        w.bool32(self.on_landscape);
        w.u32(self.landscape_material);
        w.bool32(self.reported_has_contact);
        write_plane(w, &self.reported_contact_plane);
        w.bool32(self.reported_in_water);
        w.u32(self.reported_water_material);
        w.bool32(self.reported_on_landscape);
        w.u32(self.reported_landscape_material);
        w.bool32(self.env_properties_updated);
        write_packs(w, &self.water_properties);
        write_packs(w, &self.surface_properties);
        write_packs(w, &self.env_properties);
        w.bool32(self.resolved);
        w.u32(self.water_terrain);
        w.u32(self.land_terrain);
        w.u32(self.prev_terrain);
        write_table_header(w, self.pushers.len());
        for (k, p) in &self.pushers {
            w.u32(*k);
            p.pack(w);
        }
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            desired_physics: PhysicsTypes::unpack(r, ctx)?,
            current: CollisionSettings::unpack(r, ctx)?,
            collide_mask: PhysicsTypes::unpack(r, ctx)?,
            contacts: read_table(r, |r| Ok((r.u64()?, ObjectContact::unpack(r, ctx)?)))?,
            property_lists: read_table(r, |r| Ok((r.u32()?, ContactProperties::unpack(r, ctx)?)))?,
            next_property_id: r.u32()?,
            has_normal: r.bool32()?,
            normal: read_vec3(r)?,
            impact_speed: r.f32()?,
            has_contact: r.bool32()?,
            contact_plane: read_plane(r)?,
            in_water: r.bool32()?,
            water_material: r.u32()?,
            on_landscape: r.bool32()?,
            landscape_material: r.u32()?,
            reported_has_contact: r.bool32()?,
            reported_contact_plane: read_plane(r)?,
            reported_in_water: r.bool32()?,
            reported_water_material: r.u32()?,
            reported_on_landscape: r.bool32()?,
            reported_landscape_material: r.u32()?,
            env_properties_updated: r.bool32()?,
            last_check_time: 0.0,
            water_properties: read_packs(r, ctx)?,
            surface_properties: read_packs(r, ctx)?,
            env_properties: read_packs(r, ctx)?,
            resolved: r.bool32()?,
            water_terrain: r.u32()?,
            land_terrain: r.u32()?,
            prev_terrain: r.u32()?,
            pushers: read_table(r, |r| Ok((r.u32()?, Push::unpack(r, ctx)?)))?,
        })
    }
}
