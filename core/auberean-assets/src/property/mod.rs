//! Properties: named, typed values that many records carry (entities, cells, landblocks, lights,
//! physics materials).
//!
//! A property is a property id and a value. A record stores no value type: the value is read by
//! the type the install's property list ([`PropertyList`], record `0x34000000`) declares for the
//! property id, which is why decoding needs a [`Context`]. The list also says each property's
//! group, a data word (the enum map an enum's names are in, the bit a truth value stands for),
//! how the game logic hears of it and whether it is required.
//!
//! Properties come in collections: a counted list of groups, each a group word and a counted
//! list of properties. A property is found in a collection through its group: the list names the
//! group, the collection the group, the group the property. Besides the stored (required)
//! properties a group in memory holds optional ones, which the game adds from the list (every
//! property of a group) and which are never stored.

use std::collections::HashMap;

use auberean_dat::reader::Result;
use auberean_dat::{QualifiedIds, Reader, Writer};

use crate::strings::EnumMap;
use crate::{Context, Decode, Encode};

mod value;

pub use value::{
    format_g, frame_is_valid, PropertyValue, StringInfo, StringVariable, ValueKind, VariableValue,
};

/// The enum mapping (in the master id map) whose value [`MASTER_PROPERTY_VALUE`] names the
/// property list.
pub const MASTER_PROPERTY_MAPPING: u32 = 0x4000_0002;
/// The value the property list has in [`MASTER_PROPERTY_MAPPING`].
pub const MASTER_PROPERTY_VALUE: u32 = 0x4000_0003;
/// The enumeration of the value types' names.
pub const VALUE_TYPE_NAMES: u32 = 0x4000_0013;
/// The enumeration of the property groups' names.
pub const GROUP_NAMES: u32 = 0x4000_0014;

/// Where a property's value-as-text finds the names of things: an enum's value, a record's file
/// name, a script package's name. Each answers `None` when it does not know.
pub trait Names {
    /// The name of `value` in the enum map record `map`.
    fn enum_name(&self, _map: u32, _value: u32) -> Option<String> {
        None
    }
    /// The file name of a record.
    fn file_name(&self, _id: u32) -> Option<String> {
        None
    }
    /// The name of a script package.
    // D2-13050
    fn package_name(&self, _id: u32) -> Option<String> {
        None
    }
    /// The name of a value of an enumeration (`enumeration` names the enum map through the enum
    /// maps' own mapping).
    fn enumeration_name(&self, _enumeration: u32, _value: u32) -> Option<String> {
        None
    }
}

/// [`Names`] that knows no name.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoNames;

impl Names for NoNames {}

// ------------------------------------------------------------------------------- the list

/// What the property list says about one property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertyDescriptor {
    /// The value type number (see [`ValueKind::from_number`]).
    pub value_kind: u32,
    pub group: u32,
    /// The enum map an enum property's names are in; the bit a truth value stands for.
    pub data: u32,
    /// How the game logic hears of the property: 0 reported, 1 asked for, 2 ignored.
    pub report: u32,
    pub required: bool,
}

impl Default for PropertyDescriptor {
    /// No type, group or data; asked for; not required. (A descriptor has no default, least or
    /// greatest value: the list stores none.)
    // D2-16251, D2-16252, D2-16253, D2-16257, D2-16258
    fn default() -> Self {
        Self {
            value_kind: 0,
            group: 0,
            data: 0,
            report: 1,
            required: false,
        }
    }
}

impl PropertyDescriptor {
    /// The value type, the group, the data word, the report mode and the required flag, a word
    /// each.
    // D2-16259
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            value_kind: r.u32()?,
            group: r.u32()?,
            data: r.u32()?,
            report: r.u32()?,
            required: r.bool32()?,
        })
    }

    // D2-16259
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.value_kind);
        w.u32(self.group);
        w.u32(self.data);
        w.u32(self.report);
        w.bool32(self.required);
    }

    /// The value's kind, when the declared type is one.
    #[must_use]
    pub fn kind(&self) -> Option<ValueKind> {
        ValueKind::from_number(self.value_kind)
    }

    /// The name of an enum property's value, from the enum map the descriptor names; `None` for
    /// a property that is not an enum.
    // D2-16254
    #[must_use]
    pub fn enum_name(&self, value: u32, names: &dyn Names) -> Option<String> {
        if self.value_kind != ValueKind::Enum.number() {
            return None;
        }
        names.enum_name(self.data, value)
    }

    /// Give a new property its default value: the list holds none, so nothing changes.
    // D2-16255
    pub fn init_property(&self, _property: &mut Property) -> bool {
        true
    }

    /// The records the descriptor names: an enum property's enum map.
    // D2-16256
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        if self.value_kind == ValueKind::Enum.number() {
            out.add(self.data);
        }
    }
}

/// The install's list of every property (`.mpr`, record `0x34000000`): the property names, and a
/// descriptor per property id, in the order the list stores them.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PropertyList {
    pub names: EnumMap,
    /// The size place the descriptor table was stored with.
    pub buckets: u8,
    entries: Vec<(u32, PropertyDescriptor)>,
    index: HashMap<u32, usize>,
}

impl PropertyList {
    /// A list of these descriptors, in this order, with no names.
    // D2-16249, D2-16244, D2-16242, D2-16243, D2-16234
    #[must_use]
    pub fn from_descriptors(entries: impl IntoIterator<Item = (u32, PropertyDescriptor)>) -> Self {
        let mut l = Self::default();
        for (id, d) in entries {
            l.insert(id, d);
        }
        l
    }

    fn insert(&mut self, id: u32, d: PropertyDescriptor) {
        if let Some(&i) = self.index.get(&id) {
            self.entries[i].1 = d;
        } else {
            self.index.insert(id, self.entries.len());
            self.entries.push((id, d));
        }
    }

    /// The descriptor of a property id.
    // D2-16227
    #[must_use]
    pub fn descriptor(&self, id: u32) -> Option<&PropertyDescriptor> {
        self.index.get(&id).map(|&i| &self.entries[i].1)
    }

    /// Every property id and its descriptor, in stored order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &PropertyDescriptor)> + '_ {
        self.entries.iter().map(|(k, d)| (*k, d))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The declared value type number of a property.
    // D2-16232, D2-16238
    #[must_use]
    pub fn value_type(&self, id: u32) -> Option<u32> {
        self.descriptor(id).map(|d| d.value_kind)
    }

    /// The group a property belongs to.
    // D2-16233, D2-16239
    #[must_use]
    pub fn group(&self, id: u32) -> Option<u32> {
        self.descriptor(id).map(|d| d.group)
    }

    /// The bit a truth-value property stands for (its data word); `None` for a property of any
    /// other type.
    // D2-16228, D2-16077
    #[must_use]
    pub fn bit(&self, id: u32) -> Option<u32> {
        self.descriptor(id)
            .filter(|d| d.value_kind == ValueKind::Bool.number())
            .map(|d| d.data)
    }

    /// Whether the game logic is told of a property (report mode 0); `false` for one not
    /// listed.
    // D2-16230, D2-16240, D2-16065
    #[must_use]
    pub fn is_reported(&self, id: u32) -> bool {
        self.descriptor(id).is_some_and(|d| d.report == 0)
    }

    /// Whether the game logic may ask for a property (report mode not 2); `false` for one not
    /// listed.
    // D2-16231, D2-16241, D2-16066
    #[must_use]
    pub fn is_inquirable(&self, id: u32) -> bool {
        self.descriptor(id).is_some_and(|d| d.report != 2)
    }

    /// A property's id by its name (letter case ignored).
    // D2-16186
    #[must_use]
    pub fn id_of(&self, name: &str) -> Option<u32> {
        self.names.enum_of(name)
    }

    /// A property's name.
    // D2-16187
    #[must_use]
    pub fn name_of(&self, id: u32) -> Option<&str> {
        self.names.name(id)
    }

    /// Add to `collection`, as optional properties, every property of `group` the list holds
    /// (in stored order), each with its default value; properties the collection already holds
    /// as optional are left as they are.
    // D2-16229, D2-16224
    pub fn add_optional_properties(&self, group: u32, collection: &mut PropertyCollection) -> bool {
        for (id, d) in self.iter() {
            if d.group != group {
                continue;
            }
            let mut p = Property::default();
            if p.set_name(self, id) && d.init_property(&mut p) {
                collection.add_optional_property(self, p);
            }
        }
        true
    }

    /// The records the list names: every enum property's enum map.
    // D2-16225
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for (_, d) in self.iter() {
            d.sub_ids(out);
        }
    }
}

impl Encode for PropertyList {
    /// Write the list as [`PropertyList::decode`] reads it.
    // D2-16188, D2-16246
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        self.names.write(w);
        w.hash_table(self.buckets, &self.entries, |w, (k, d)| {
            w.u32(*k);
            d.write(w);
        });
    }
}

impl Decode for PropertyList {
    /// The id, the names (an enum map with its own id), then a hash table of descriptors keyed by
    /// property id.
    // D2-16188, D2-16246, D2-16245, D2-05057, D2-16248, D2-16250
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let names = EnumMap::read(r)?;
        let (buckets, entries) = r.hash_table_with_buckets(24, |r| {
            let key = r.u32()?;
            Ok((key, PropertyDescriptor::read(r)?))
        })?;
        let mut l = Self::from_descriptors(entries);
        l.names = names;
        l.buckets = buckets;
        Ok(l)
    }
}

// ------------------------------------------------------------------------------- one property

/// One property: its id, its declared type and group, and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub id: u32,
    /// The declared value type number (0 before the property is named).
    pub value_type: u32,
    pub group: u32,
    pub value: PropertyValue,
}

impl Default for Property {
    /// A property with no name, type or group, holding a false truth value until named.
    // D2-16073, D2-16074, D2-16075, D2-16080
    fn default() -> Self {
        Self {
            id: 0,
            value_type: 0,
            group: 0,
            value: PropertyValue::Bool(false),
        }
    }
}

impl Property {
    /// A property named `id`, typed and grouped by the list, holding its type's new value.
    #[must_use]
    pub fn named(list: &PropertyList, id: u32) -> Option<Self> {
        let mut p = Self::default();
        p.set_name(list, id).then_some(p)
    }

    /// Name the property: its type and group from the list, and a new value of its type.
    /// `false` when the list does not hold the id or declares no type the game knows.
    // D2-16076
    pub fn set_name(&mut self, list: &PropertyList, id: u32) -> bool {
        self.id = id;
        let (Some(ty), Some(group)) = (list.value_type(id), list.group(id)) else {
            return false;
        };
        self.group = group;
        if !self.set_type(ty) {
            return false;
        }
        self.init_to_default();
        true
    }

    /// Name the property by its name (letter case ignored).
    // D2-16081
    pub fn set_name_text(&mut self, list: &PropertyList, name: &str) -> bool {
        list.id_of(name).is_some_and(|id| self.set_name(list, id))
    }

    /// Set the declared type; a new type replaces the value with a new value of it. `false`
    /// when the type is not one the game knows.
    // D2-16071
    pub fn set_type(&mut self, ty: u32) -> bool {
        if self.value_type == ty {
            return true;
        }
        self.value_type = ty;
        match ValueKind::from_number(ty) {
            Some(k) => {
                self.value = PropertyValue::new(k);
                true
            }
            None => false,
        }
    }

    /// Reset the value to its type's default: the list stores no defaults, so the value becomes
    /// a new value of its type and the answer is `false` (no default was applied).
    // D2-16068
    pub fn init_to_default(&mut self) -> bool {
        if self.value_type == 0 {
            return false;
        }
        if let Some(k) = ValueKind::from_number(self.value_type) {
            self.value = PropertyValue::new(k);
        }
        false
    }

    /// Whether the value is the list's default for the property: the list stores none, so never.
    // D2-16067
    #[must_use]
    pub fn is_default(&self) -> bool {
        false
    }

    /// The group the property belongs to.
    // D2-16070
    #[must_use]
    pub fn group(&self) -> u32 {
        self.group
    }

    /// Whether two properties are the same: the same id and type, and the same value.
    // D2-16078
    #[must_use]
    pub fn same(&self, o: &Property) -> bool {
        self.id == o.id && self.value_type == o.value_type && self.value.same(&o.value)
    }

    /// The property's name.
    // D2-16082
    #[must_use]
    pub fn name<'a>(&self, list: &'a PropertyList) -> Option<&'a str> {
        list.name_of(self.id)
    }

    /// The name of the property's value type.
    // D2-16083
    #[must_use]
    pub fn type_name(&self, names: &dyn Names) -> Option<String> {
        names.enumeration_name(VALUE_TYPE_NAMES, self.value_type)
    }

    /// A string property's value as text.
    // D2-08369
    #[must_use]
    pub fn string_value(&self) -> Option<&str> {
        self.value.inq_string()
    }

    /// The records the property names: a file property's record, a reference's string tables.
    // D2-16072
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        if self.value_type == ValueKind::DataFile.number()
            || self.value_type == ValueKind::StringInfo.number()
        {
            self.value.sub_ids(out);
        }
    }

    /// The property id, then its value by the type the property list declares.
    // D2-16079
    pub fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let at = r.pos();
        let id = r.u32()?;
        let Some(list) = ctx.properties() else {
            return r.fail("no property list in the context");
        };
        let Some(desc) = list.descriptor(id) else {
            return Err(auberean_dat::DecodeError {
                at,
                what: format!("property {id:#x} is not in the property list"),
            });
        };
        let Some(kind) = ValueKind::from_number(desc.value_kind) else {
            return r.fail(format!(
                "property {id:#x} declares value type {:#x}",
                desc.value_kind
            ));
        };
        Ok(Self {
            id,
            value_type: desc.value_kind,
            group: desc.group,
            value: PropertyValue::read(r, kind)?,
        })
    }

    // D2-16079
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.id);
        self.value.write(w);
    }
}

// ------------------------------------------------------------------------------- groups

/// A group of properties: the stored (required) ones, and the optional ones the game adds in
/// memory.
// D2-16093, D2-16094, D2-16085, D2-16105
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PropertyGroup {
    pub group: u32,
    pub properties: Vec<Property>,
    pub optional: Vec<Property>,
}

impl PropertyGroup {
    /// An empty group of a given name.
    #[must_use]
    pub fn new(group: u32) -> Self {
        Self {
            group,
            ..Self::default()
        }
    }

    /// The group word, then a counted list of properties.
    // D2-16087, D2-04377
    pub fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let group = r.u32()?;
        let properties = r.list(4, |r| Property::read(r, ctx))?;
        Ok(Self {
            group,
            properties,
            optional: Vec::new(),
        })
    }

    // D2-16087, D2-04377
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.group);
        w.list(&self.properties, |w, p| p.write(w));
    }

    // D2-16089
    #[must_use]
    pub fn get(&self, id: u32) -> Option<&Property> {
        self.properties.iter().find(|p| p.id == id)
    }

    // D2-16089
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Property> {
        self.properties.iter_mut().find(|p| p.id == id)
    }

    // D2-16090
    #[must_use]
    pub fn get_optional(&self, id: u32) -> Option<&Property> {
        self.optional.iter().find(|p| p.id == id)
    }

    /// A copy of a stored property.
    // D2-16091
    #[must_use]
    pub fn inq(&self, id: u32) -> Option<Property> {
        self.get(id).cloned()
    }

    // D2-16092
    #[must_use]
    pub fn inq_optional(&self, id: u32) -> Option<Property> {
        self.get_optional(id).cloned()
    }

    /// A copy of the stored property at a place.
    #[must_use]
    pub fn inq_at(&self, n: usize) -> Option<Property> {
        self.properties.get(n).cloned()
    }

    #[must_use]
    pub fn inq_optional_at(&self, n: usize) -> Option<Property> {
        self.optional.get(n).cloned()
    }

    /// The group's name.
    // D2-16097
    #[must_use]
    pub fn name(&self, names: &dyn Names) -> Option<String> {
        names.enumeration_name(GROUP_NAMES, self.group)
    }

    // D2-16098
    #[must_use]
    pub fn contains(&self, id: u32) -> bool {
        self.get(id).is_some()
    }

    // D2-16099
    #[must_use]
    pub fn contains_optional(&self, id: u32) -> bool {
        self.get_optional(id).is_some()
    }

    /// Make the optional properties the stored ones, when the group stores none; `false`
    /// otherwise.
    // D2-16100
    pub fn set_optional_to_required(&mut self) -> bool {
        if !self.properties.is_empty() {
            return false;
        }
        self.properties = std::mem::take(&mut self.optional);
        true
    }

    /// Add a stored property; `false` when the group already stores one of its id.
    // D2-16101
    pub fn add(&mut self, p: Property) -> bool {
        if self.contains(p.id) {
            return false;
        }
        self.properties.push(p);
        true
    }

    /// Add an optional property; `false` when the group already holds an optional one of its id.
    // D2-16102
    pub fn add_optional(&mut self, p: Property) -> bool {
        if self.contains_optional(p.id) {
            return false;
        }
        self.optional.push(p);
        true
    }

    /// Remove a stored property; `false` when the group stores none of that id.
    // D2-16088
    pub fn remove(&mut self, id: u32) -> bool {
        match self.properties.iter().position(|p| p.id == id) {
            Some(i) => {
                self.properties.remove(i);
                true
            }
            None => false,
        }
    }

    /// Set a stored property's value from `p`'s (asking `p` for the stored one's type), or add
    /// `p` when the group stores no property of its id.
    // D2-16103
    pub fn update(&mut self, p: &Property) -> bool {
        if let Some(mine) = self.get_mut(p.id) {
            mine.value.set_from(&p.value);
            return true;
        }
        self.properties.push(p.clone());
        true
    }

    /// Update the group with every stored property of another.
    // D2-16104
    pub fn update_group(&mut self, o: &PropertyGroup) -> bool {
        for p in &o.properties {
            self.update(p);
        }
        true
    }

    /// The records the group's stored properties name.
    // D2-16086
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for p in &self.properties {
            p.sub_ids(out);
        }
    }
}

// ------------------------------------------------------------------------------- collections

/// A record's properties, in groups.
// D2-16115, D2-16116, D2-16106, D2-16126
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PropertyCollection {
    pub groups: Vec<PropertyGroup>,
}

impl PropertyCollection {
    /// A counted list of groups.
    // D2-04377, D2-16087, D2-04378
    pub fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let groups = r.list(8, |r| PropertyGroup::read(r, ctx))?;
        Ok(Self { groups })
    }

    // D2-04377, D2-04378
    pub fn write(&self, w: &mut Writer) {
        w.list(&self.groups, |w, g| g.write(w));
    }

    /// Every stored property, across the groups.
    pub fn iter(&self) -> impl Iterator<Item = &Property> + '_ {
        self.groups.iter().flat_map(|g| g.properties.iter())
    }

    /// How many stored properties the collection holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.groups.iter().map(|g| g.properties.len()).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The group of a name.
    // D2-16109
    #[must_use]
    pub fn group(&self, group: u32) -> Option<&PropertyGroup> {
        self.groups.iter().find(|g| g.group == group)
    }

    fn group_mut(&mut self, group: u32) -> Option<&mut PropertyGroup> {
        self.groups.iter_mut().find(|g| g.group == group)
    }

    /// A stored property, found through the group the list puts it in; `None` when the list
    /// does not hold the property or puts it in group 0.
    // D2-16110
    #[must_use]
    pub fn get(&self, list: &PropertyList, id: u32) -> Option<&Property> {
        let group = list.group(id).filter(|g| *g != 0)?;
        self.group(group)?.get(id)
    }

    /// An optional property, found as [`PropertyCollection::get`] finds a stored one.
    // D2-16111
    #[must_use]
    pub fn get_optional(&self, list: &PropertyList, id: u32) -> Option<&Property> {
        let group = list.group(id).filter(|g| *g != 0)?;
        self.group(group)?.get_optional(id)
    }

    /// A copy of the group at a place.
    #[must_use]
    pub fn inq_group_at(&self, n: usize) -> Option<PropertyGroup> {
        self.groups.get(n).cloned()
    }

    // D2-16113
    #[must_use]
    pub fn inq(&self, list: &PropertyList, id: u32) -> Option<Property> {
        self.get(list, id).cloned()
    }

    // D2-16114
    #[must_use]
    pub fn inq_optional(&self, list: &PropertyList, id: u32) -> Option<Property> {
        self.get_optional(list, id).cloned()
    }

    /// A stored property by its name (letter case ignored).
    // D2-16120
    #[must_use]
    pub fn inq_by_name(&self, list: &PropertyList, name: &str) -> Option<Property> {
        self.inq(list, list.id_of(name)?)
    }

    // D2-16118
    #[must_use]
    pub fn contains_group(&self, group: u32) -> bool {
        self.group(group).is_some()
    }

    // D2-16119
    #[must_use]
    pub fn contains(&self, list: &PropertyList, id: u32) -> bool {
        self.get(list, id).is_some()
    }

    /// Add a group, unless the collection already has one of its name (which counts as done).
    // D2-16123
    pub fn add_group(&mut self, g: PropertyGroup) -> bool {
        if !self.contains_group(g.group) {
            self.groups.push(g);
        }
        true
    }

    /// Update the group of `g`'s name with `g`'s stored properties, or add `g`.
    // D2-16124
    pub fn update_group(&mut self, g: &PropertyGroup) -> bool {
        if let Some(mine) = self.group_mut(g.group) {
            return mine.update_group(g);
        }
        self.add_group(g.clone())
    }

    /// Update with every group of another collection.
    // D2-16125
    pub fn update_collection(&mut self, o: &PropertyCollection) -> bool {
        let mut ok = true;
        for g in &o.groups {
            ok &= self.update_group(g);
        }
        ok
    }

    /// Add a stored property to its group (made when missing); `false` when the collection
    /// already stores one of its id, or the group already stores it.
    // D2-16127
    pub fn add_property(&mut self, list: &PropertyList, p: Property) -> bool {
        if self.get(list, p.id).is_some() {
            return false;
        }
        if let Some(g) = self.group_mut(p.group) {
            return g.add(p);
        }
        let mut g = PropertyGroup::new(p.group);
        g.add(p);
        self.add_group(g)
    }

    /// Add an optional property to its group (made when missing).
    // D2-16128
    pub fn add_optional_property(&mut self, list: &PropertyList, p: Property) -> bool {
        if self.get_optional(list, p.id).is_some() {
            return false;
        }
        if let Some(g) = self.group_mut(p.group) {
            return g.add_optional(p);
        }
        let mut g = PropertyGroup::new(p.group);
        g.add_optional(p);
        self.add_group(g)
    }

    /// Update a stored property in its group, or add it.
    // D2-16129
    pub fn update_property(&mut self, list: &PropertyList, p: &Property) -> bool {
        if let Some(g) = self.group_mut(p.group) {
            return g.update(p);
        }
        self.add_property(list, p.clone())
    }

    /// Make each group's optional properties its stored ones; `false` at the first group that
    /// already stores some.
    // D2-16107
    pub fn set_optional_to_required(&mut self) -> bool {
        self.groups
            .iter_mut()
            .all(PropertyGroup::set_optional_to_required)
    }

    /// The records the collection's stored properties name.
    // D2-16108
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for g in &self.groups {
            g.sub_ids(out);
        }
    }
}

/// A physics material (`.pmat`): the id and a property collection.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PhysicsMaterial {
    pub properties: PropertyCollection,
}

impl Decode for PhysicsMaterial {
    // D2-13695
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            properties: PropertyCollection::read(r, ctx)?,
        })
    }
}

impl Encode for PhysicsMaterial {
    // D2-13695
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        self.properties.write(w);
    }
}
