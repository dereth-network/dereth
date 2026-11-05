//! Collisions as a body reports them: the objects it touches and stops touching, the properties of
//! what it touches (another object's, the water's, the ground's, its surroundings'), the pushes
//! those properties give, and what it passes through.
//!
//! A body keeps one [`Collisions`]. Each step the transition tells it what it touched; at the end of
//! the step [`Collisions::report_collision`] fills a [`CollisionReport`] with what began and what
//! ended. An object touched and then not touched for more than a second ends; so does everything
//! when the body leaves the world.

pub mod manager;

use auberean_assets::property::{Property, PropertyList, PropertyValue};
use auberean_assets::Context;
use auberean_dat::reader::Result;
use auberean_dat::Reader;
use dereth_primitives::shape::Plane;
use dereth_primitives::Vec3;

use crate::ethereal::{ObjectEthereality, PhysicsTypes};
use crate::pack::{
    read_ids, read_packs, read_plane, read_vec3, write_ids, write_packs, write_plane, write_vec3,
    Pack, Writer,
};

pub use manager::{CollisionHost, Collisions};

/// How a contact property changed since it was last reported.
pub mod change {
    /// Newly touched.
    pub const BEGIN: u32 = 0;
    /// No longer touched.
    pub const END: u32 = 1;
    /// Touched still, with a new value.
    pub const CHANGED: u32 = 2;
    /// Touched still, unchanged.
    pub const SAME: u32 = 3;
}

/// Two properties are the same: the same id and type, and values the same, floats in a vector,
/// color or position within a tolerance ([`Property::same`]).
#[must_use]
pub fn same_property(a: &Property, b: &Property) -> bool {
    a.same(b)
}

/// One property of something a body touches, numbered, with how it changed.
// D2-15586, D2-04254
#[derive(Clone, Debug, PartialEq)]
pub struct ContactProperty {
    pub id: u32,
    /// See [`change`].
    pub change: u32,
    pub property: Property,
}

impl ContactProperty {
    /// A property newly touched.
    // D2-15585, D2-15587
    #[must_use]
    pub fn new(property: Property, id: u32) -> Self {
        Self {
            id,
            change: change::BEGIN,
            property,
        }
    }

    /// The property's id.
    // D2-15575
    #[must_use]
    pub fn name(&self) -> u32 {
        self.property.id
    }

    #[must_use]
    pub fn is_begin(&self) -> bool {
        self.change == change::BEGIN
    }

    // D2-15577
    #[must_use]
    pub fn is_end(&self) -> bool {
        self.change == change::END
    }

    // D2-15579
    #[must_use]
    pub fn is_same(&self) -> bool {
        self.change == change::SAME
    }

    // D2-15581
    pub fn set_as_begin(&mut self) {
        self.change = change::BEGIN;
    }

    // D2-15580
    pub fn set_as_end(&mut self) {
        self.change = change::END;
    }

    // D2-15582
    pub fn set_as_change(&mut self) {
        self.change = change::CHANGED;
    }

    // D2-15583
    pub fn set_as_same(&mut self) {
        self.change = change::SAME;
    }

    // D2-15584
    #[must_use]
    pub fn is_reported(&self, list: &PropertyList) -> bool {
        list.is_reported(self.name())
    }

    /// Takes a newly touched value: the same as held marks it unchanged; a different value of the
    /// same kind is taken and marks it changed; a value of another kind is ignored.
    // D2-15588
    pub fn update_value(&mut self, new: &Property) {
        if same_property(&self.property, new) {
            self.change = change::SAME;
        } else if std::mem::discriminant(&self.property.value) == std::mem::discriminant(&new.value)
        {
            self.property.value = new.value.clone();
            self.change = change::CHANGED;
        }
    }

    /// The value of a vector property.
    #[must_use]
    pub fn vector(&self) -> Option<Vec3> {
        match &self.property.value {
            PropertyValue::Vector(v) => Some(Vec3::new(v.x, v.y, v.z)),
            _ => None,
        }
    }

    /// The value of a flag property.
    #[must_use]
    pub fn flag(&self) -> Option<bool> {
        match &self.property.value {
            PropertyValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The value of an enumerated property.
    #[must_use]
    pub fn enumeration(&self) -> Option<u32> {
        match &self.property.value {
            PropertyValue::Enum(e) => Some(*e),
            _ => None,
        }
    }
}

impl Pack for ContactProperty {
    // D2-15590
    fn pack(&self, w: &mut Writer) {
        w.u32(self.id);
        w.u32(self.change);
        self.property.write(w);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            id: r.u32()?,
            change: r.u32()?,
            property: Property::read(r, ctx)?,
        })
    }
}

/// The properties of one object a body touches, keyed by the body's handle for that object.
// D2-15570, D2-15571, D2-06752, D2-06763
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContactProperties {
    /// The handle the body keeps the object under.
    pub key: u32,
    /// The object's id; 0 for scenery, which has none.
    pub object: u64,
    /// When the object was last touched.
    pub touched_time: f64,
    pub reported: bool,
    pub remove_now: bool,
    pub properties: Vec<ContactProperty>,
}

impl ContactProperties {
    // D2-15569
    #[must_use]
    pub fn get(&self, i: usize) -> Option<&ContactProperty> {
        self.properties.get(i)
    }

    // D2-15572
    pub fn add(&mut self, p: ContactProperty) {
        self.properties.push(p);
    }

    /// Each reported property begins, and is reported as an object property.
    // D2-15573
    pub fn begin_property_collision(&mut self, report: &mut CollisionReport, list: &PropertyList) {
        for p in &mut self.properties {
            if p.is_reported(list) {
                p.set_as_begin();
                report.object_properties.push(p.clone());
            }
        }
    }

    /// Each reported property ends, and is reported as an object property.
    // D2-15574
    pub fn end_property_collision(&mut self, report: &mut CollisionReport, list: &PropertyList) {
        for p in &mut self.properties {
            if p.is_reported(list) {
                p.set_as_end();
                report.object_properties.push(p.clone());
            }
        }
    }
}

impl Pack for ContactProperties {
    // D2-15568
    fn pack(&self, w: &mut Writer) {
        w.u32(self.key);
        w.u64(self.object);
        w.f64(self.touched_time);
        w.bool32(self.reported);
        w.bool32(self.remove_now);
        write_packs(w, &self.properties);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            key: r.u32()?,
            object: r.u64()?,
            touched_time: r.f64()?,
            reported: r.bool32()?,
            remove_now: r.bool32()?,
            properties: read_packs(r, ctx)?,
        })
    }
}

/// One object touched or let go of, as reported.
// D2-04257
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactRecord {
    /// The object's id; 0 for none.
    pub object: u64,
    /// True when the touch began, false when it ended.
    pub begin: bool,
    /// Whether the body passes through the object.
    pub ethereal: bool,
}

impl Default for ContactRecord {
    // D2-04255
    fn default() -> Self {
        Self {
            object: 0,
            begin: true,
            ethereal: false,
        }
    }
}

impl Pack for ContactRecord {
    // D2-04256
    fn pack(&self, w: &mut Writer) {
        w.u64(self.object);
        w.bool32(self.begin);
        w.bool32(self.ethereal);
    }

    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        Ok(Self {
            object: r.u64()?,
            begin: r.bool32()?,
            ethereal: r.bool32()?,
        })
    }
}

/// An object a body is touching: when it was last touched, whether that has been reported,
/// whether the body ignores it.
// D2-15566, D2-06742
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ObjectContact {
    pub touched_time: f64,
    pub reported: bool,
    pub remove_now: bool,
    pub ignore: bool,
    /// Whether the ignoring changed after the touch was reported.
    pub ignore_changed: bool,
    pub record: ContactRecord,
}

impl ObjectContact {
    // D2-15563
    #[must_use]
    pub fn id(&self) -> u64 {
        self.record.object
    }

    // D2-15564
    pub fn set_as_end_collide(&mut self) {
        self.record.begin = false;
    }

    // D2-15565
    pub fn set_as_begin_collide(&mut self) {
        self.record.begin = true;
    }

    // D2-06715
    pub fn update_ignore_collide(&mut self, ignore: bool) {
        self.ignore = ignore;
        if self.reported {
            self.ignore_changed = true;
        }
    }
}

impl Pack for ObjectContact {
    // D2-06743
    fn pack(&self, w: &mut Writer) {
        w.f64(self.touched_time);
        w.bool32(self.reported);
        w.bool32(self.remove_now);
        self.record.pack(w);
        w.bool32(self.ignore);
        w.bool32(self.ignore_changed);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let touched_time = r.f64()?;
        let reported = r.bool32()?;
        let remove_now = r.bool32()?;
        let record = ContactRecord::unpack(r, ctx)?;
        Ok(Self {
            touched_time,
            reported,
            remove_now,
            record,
            ignore: r.bool32()?,
            ignore_changed: r.bool32()?,
        })
    }
}

/// The state of a reported collision normal or contact plane.
pub mod state {
    pub const BEGIN: u32 = 0;
    pub const END: u32 = 1;
    pub const CHANGED: u32 = 2;
    /// Nothing to report.
    pub const NONE: u32 = 3;
}

/// What one step's collisions reported: the objects touched and let go of, the properties of what
/// was touched, the collision normal and the contact plane.
// D2-15557, D2-04304, D2-15562
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionReport {
    pub records: Vec<ContactRecord>,
    pub object_properties: Vec<ContactProperty>,
    pub env_properties: Vec<ContactProperty>,
    pub surface_properties: Vec<ContactProperty>,
    pub water_properties: Vec<ContactProperty>,
    /// See [`state`].
    pub normal_state: u32,
    pub contact_state: u32,
    pub normal: Vec3,
    pub impact_speed: f32,
    pub contact_plane: Plane,
}

impl Default for CollisionReport {
    // D2-15556
    fn default() -> Self {
        Self {
            records: Vec::new(),
            object_properties: Vec::new(),
            env_properties: Vec::new(),
            surface_properties: Vec::new(),
            water_properties: Vec::new(),
            normal_state: state::NONE,
            contact_state: state::NONE,
            normal: Vec3::ZERO,
            impact_speed: 0.0,
            contact_plane: Plane::default(),
        }
    }
}

impl CollisionReport {
    /// The collision normal and impact speed, when a collision began or changed.
    // D2-15544
    #[must_use]
    pub fn collided(&self) -> Option<(Vec3, f32)> {
        matches!(self.normal_state, state::BEGIN | state::CHANGED)
            .then_some((self.normal, self.impact_speed))
    }

    // D2-15545
    #[must_use]
    pub fn is_end_collision(&self) -> bool {
        self.normal_state == state::END
    }

    /// The contact plane, when a contact began or changed.
    // D2-15546
    #[must_use]
    pub fn contact(&self) -> Option<Plane> {
        matches!(self.contact_state, state::BEGIN | state::CHANGED).then_some(self.contact_plane)
    }

    #[must_use]
    pub fn is_end_contact(&self) -> bool {
        self.contact_state == state::END
    }

    // D2-15548, D2-15549
    #[must_use]
    pub fn record(&self, i: usize) -> Option<&ContactRecord> {
        self.records.get(i)
    }

    #[must_use]
    pub fn object_property(&self, i: usize) -> Option<&ContactProperty> {
        self.object_properties.get(i)
    }

    #[must_use]
    pub fn surface_property(&self, i: usize) -> Option<&ContactProperty> {
        self.surface_properties.get(i)
    }

    #[must_use]
    pub fn water_property(&self, i: usize) -> Option<&ContactProperty> {
        self.water_properties.get(i)
    }

    /// The first record of an object.
    #[must_use]
    pub fn find_object(&self, object: u64) -> Option<&ContactRecord> {
        self.records.iter().find(|r| r.object == object)
    }

    /// Whether there is anything to report.
    // D2-15555
    #[must_use]
    pub fn non_empty(&self) -> bool {
        !(self.records.is_empty()
            && self.object_properties.is_empty()
            && self.env_properties.is_empty()
            && self.surface_properties.is_empty()
            && self.water_properties.is_empty()
            && self.normal_state == state::NONE
            && self.contact_state == state::NONE)
    }

    // D2-15560
    pub fn add_as_begin_collision(&mut self, c: &mut ObjectContact) {
        c.set_as_begin_collide();
        self.records.push(c.record);
    }

    // D2-15561
    pub fn add_as_end_collision(&mut self, c: &mut ObjectContact) {
        c.set_as_end_collide();
        self.records.push(c.record);
    }
}

impl Pack for CollisionReport {
    // D2-15558, D2-04334, D2-04357
    fn pack(&self, w: &mut Writer) {
        write_packs(w, &self.records);
        write_packs(w, &self.object_properties);
        write_packs(w, &self.env_properties);
        write_packs(w, &self.surface_properties);
        write_packs(w, &self.water_properties);
        w.u32(self.normal_state);
        w.u32(self.contact_state);
        write_vec3(w, self.normal);
        w.f32(self.impact_speed);
        write_plane(w, &self.contact_plane);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            records: read_packs(r, ctx)?,
            object_properties: read_packs(r, ctx)?,
            env_properties: read_packs(r, ctx)?,
            surface_properties: read_packs(r, ctx)?,
            water_properties: read_packs(r, ctx)?,
            normal_state: r.u32()?,
            contact_state: r.u32()?,
            normal: read_vec3(r)?,
            impact_speed: r.f32()?,
            contact_plane: read_plane(r)?,
        })
    }
}

/// Which parts of [`CollisionSettings`] are set.
pub mod settings {
    pub const MOVEMENT_PHYSICS: u32 = 0x1;
    pub const MOVEMENT_OBJECTS: u32 = 0x2;
    pub const PLACEMENT_PHYSICS: u32 = 0x4;
    pub const PLACEMENT_OBJECTS: u32 = 0x8;
    pub const PHYSICS_TYPE: u32 = 0x10;
}

/// What a body passes through, moving and being placed, its own physics type, the objects it
/// passes through for now, and the smallest impact speeds it reports.
// D2-15539
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionSettings {
    /// See [`settings`].
    pub bits: u32,
    pub physics_type: PhysicsTypes,
    pub placement_physics: PhysicsTypes,
    pub movement_physics: PhysicsTypes,
    pub placement_objects: ObjectEthereality,
    pub movement_objects: ObjectEthereality,
    pub temp_ethereal: Vec<u64>,
    pub min_impact_xy: f32,
    pub min_impact_z: f32,
}

impl Default for CollisionSettings {
    /// Solid to every object and to no physics type.
    // D2-15538
    fn default() -> Self {
        let mut placement_objects = ObjectEthereality::default();
        placement_objects.set_to_solid();
        Self {
            bits: 0,
            physics_type: PhysicsTypes::default(),
            placement_physics: PhysicsTypes::default(),
            movement_physics: PhysicsTypes::default(),
            movement_objects: placement_objects.clone(),
            placement_objects,
            temp_ethereal: Vec::new(),
            min_impact_xy: 0.0,
            min_impact_z: 0.0,
        }
    }
}

impl CollisionSettings {
    // D2-15529
    pub fn set_bit(&mut self, bit: u32) {
        self.bits |= bit;
    }

    #[must_use]
    pub fn has(&self, bit: u32) -> bool {
        self.bits & bit != 0
    }

    fn touch(&mut self, bit: u32) {
        if self.bits & bit == 0 {
            match bit {
                settings::PLACEMENT_PHYSICS => self.placement_physics.mask = 0,
                settings::MOVEMENT_PHYSICS => self.movement_physics.mask = 0,
                _ => self.physics_type.mask = 0,
            }
            self.bits |= bit;
        }
    }

    // D2-15530
    pub fn update_placement_ethereal(&mut self, list: &PropertyList, property: u32) -> bool {
        self.touch(settings::PLACEMENT_PHYSICS);
        self.placement_physics.set_property(list, property, true)
    }

    // D2-15531
    pub fn update_placement_solid(&mut self, list: &PropertyList, property: u32) -> bool {
        self.touch(settings::PLACEMENT_PHYSICS);
        self.placement_physics.set_property(list, property, false)
    }

    // D2-15532
    pub fn update_movement_ethereal(&mut self, list: &PropertyList, property: u32) -> bool {
        self.touch(settings::MOVEMENT_PHYSICS);
        self.movement_physics.set_property(list, property, true)
    }

    // D2-15533
    pub fn update_movement_solid(&mut self, list: &PropertyList, property: u32) -> bool {
        self.touch(settings::MOVEMENT_PHYSICS);
        self.movement_physics.set_property(list, property, false)
    }

    // D2-15534
    pub fn update_physics_type(&mut self, list: &PropertyList, property: u32) -> bool {
        self.touch(settings::PHYSICS_TYPE);
        self.physics_type.set_property(list, property, true)
    }

    // D2-15535
    pub fn remove_physics_type(&mut self, list: &PropertyList, property: u32) -> bool {
        self.touch(settings::PHYSICS_TYPE);
        self.physics_type.set_property(list, property, false)
    }

    /// Passes through `object` moving and being placed. Object settings not yet set start as
    /// solid to every object, with no lists.
    // D2-15536
    pub fn add_to_ethereal_list(&mut self, object: u64) {
        let mut solid = ObjectEthereality::default();
        solid.set_to_solid();
        if !self.has(settings::PLACEMENT_OBJECTS) {
            solid.update(&mut self.placement_objects);
            self.bits |= settings::PLACEMENT_OBJECTS;
        }
        if !self.has(settings::MOVEMENT_OBJECTS) {
            solid.update(&mut self.movement_objects);
            self.bits |= settings::MOVEMENT_OBJECTS;
        }
        self.placement_objects.add_to_ethereal_list(object);
        self.movement_objects.add_to_ethereal_list(object);
    }

    // D2-15537
    #[must_use]
    pub fn find_in_temp_ethereal_list(&self, object: u64) -> Option<usize> {
        self.temp_ethereal.iter().position(|i| *i == object)
    }

    // D2-15542
    #[must_use]
    pub fn in_temp_ethereal_list(&self, object: u64) -> bool {
        self.find_in_temp_ethereal_list(object).is_some()
    }

    // D2-15543
    pub fn add_to_temp_ethereal_list(&mut self, object: u64) {
        if !self.in_temp_ethereal_list(object) {
            self.temp_ethereal.push(object);
        }
    }

    /// Removes `object`, the last entry taking its place.
    // D2-15541
    pub fn remove_from_temp_ethereal_list(&mut self, object: u64) {
        if let Some(i) = self.find_in_temp_ethereal_list(object) {
            self.temp_ethereal.swap_remove(i);
        }
    }

    // D2-05832
    pub fn set_physics_placement_type(&mut self, t: PhysicsTypes) {
        self.placement_physics = t;
        self.set_bit(settings::PLACEMENT_PHYSICS);
    }

    // D2-05833
    pub fn set_physics_movement_type(&mut self, t: PhysicsTypes) {
        self.movement_physics = t;
        self.set_bit(settings::MOVEMENT_PHYSICS);
    }

    // D2-06713
    pub fn set_weenie_placement_type(&mut self, t: &ObjectEthereality) {
        t.update(&mut self.placement_objects);
        self.set_bit(settings::PLACEMENT_OBJECTS);
    }

    // D2-06714
    pub fn set_weenie_movement_type(&mut self, t: &ObjectEthereality) {
        t.update(&mut self.movement_objects);
        self.set_bit(settings::MOVEMENT_OBJECTS);
    }
}

impl Pack for CollisionSettings {
    // D2-15540
    fn pack(&self, w: &mut Writer) {
        w.u32(self.bits);
        self.placement_physics.pack(w);
        self.movement_physics.pack(w);
        self.placement_objects.pack(w);
        self.movement_objects.pack(w);
        self.physics_type.pack(w);
        write_ids(w, &self.temp_ethereal);
        w.f32(self.min_impact_xy);
        w.f32(self.min_impact_z);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let bits = r.u32()?;
        let placement_physics = PhysicsTypes::unpack(r, ctx)?;
        let movement_physics = PhysicsTypes::unpack(r, ctx)?;
        let placement_objects = ObjectEthereality::unpack(r, ctx)?;
        let movement_objects = ObjectEthereality::unpack(r, ctx)?;
        Ok(Self {
            bits,
            placement_physics,
            movement_physics,
            placement_objects,
            movement_objects,
            physics_type: PhysicsTypes::unpack(r, ctx)?,
            temp_ethereal: read_ids(r)?,
            min_impact_xy: r.f32()?,
            min_impact_z: r.f32()?,
        })
    }
}

/// The pushes an object's properties give a body touching it: a velocity and an acceleration
/// everywhere, and one each on the ground, in the air, in water and on the sea floor; and whether
/// the body walks on air while touching it.
// D2-06732, D2-06740
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Push {
    pub walk_on_air: bool,
    /// Never set and never sent.
    pub local: bool,
    pub general_velocity: Vec3,
    pub general_acceleration: Vec3,
    pub earth_velocity: Vec3,
    pub earth_acceleration: Vec3,
    pub air_velocity: Vec3,
    pub air_acceleration: Vec3,
    pub water_velocity: Vec3,
    pub water_acceleration: Vec3,
    pub seafloor_velocity: Vec3,
    pub seafloor_acceleration: Vec3,
}

impl Push {
    fn vectors(&self) -> [Vec3; 10] {
        [
            self.general_velocity,
            self.general_acceleration,
            self.earth_velocity,
            self.earth_acceleration,
            self.air_velocity,
            self.air_acceleration,
            self.water_velocity,
            self.water_acceleration,
            self.seafloor_velocity,
            self.seafloor_acceleration,
        ]
    }
}

impl Pack for Push {
    /// The walk-on-air flag and the ten vectors; the local flag is not sent.
    // D2-06746
    fn pack(&self, w: &mut Writer) {
        w.bool32(self.walk_on_air);
        for v in self.vectors() {
            write_vec3(w, v);
        }
    }

    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        Ok(Self {
            walk_on_air: r.bool32()?,
            local: false,
            general_velocity: read_vec3(r)?,
            general_acceleration: read_vec3(r)?,
            earth_velocity: read_vec3(r)?,
            earth_acceleration: read_vec3(r)?,
            air_velocity: read_vec3(r)?,
            air_acceleration: read_vec3(r)?,
            water_velocity: read_vec3(r)?,
            water_acceleration: read_vec3(r)?,
            seafloor_velocity: read_vec3(r)?,
            seafloor_acceleration: read_vec3(r)?,
        })
    }
}
