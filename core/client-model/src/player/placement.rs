//! Local placement properties; never an immediate network request.
//! The player module's chat-option structure lookup and its chat-window-option setter.

use dereth_primitives::ServerTime;
use dereth_protocol::property::{BaseProperty, BasePropertyValue, PropertyCollection};

const ARRAY: u32 = 0x1000_008C;
const ROW: u32 = 0x1000_008B;
/// The property descriptor's default array limit. Deserializing the descriptor does not
/// overwrite it.
const MAX_WINDOWS: u32 = 0xFFFF;

fn empty_row() -> BaseProperty {
    BaseProperty {
        name: ROW,
        value: Some(BasePropertyValue::Struct(PropertyCollection {
            // A requested bucket count of zero selects the minimum of 11 buckets.
            bucket_index: 0,
            entries: Vec::new(),
        })),
    }
}

/// The property-table setter assigns even an equal value. New entries are inserted at the head
/// of their bucket, while existing entries retain their key/order and unrelated fields survive.
fn set(collection: &mut PropertyCollection, property: BaseProperty) {
    if let Some((_, old)) = collection
        .entries
        .iter_mut()
        .find(|(key, _)| *key == property.name)
    {
        *old = property;
        return;
    }
    let sizes = dereth_protocol::archive::BUCKET_SIZES;
    let index = usize::from(collection.bucket_index);
    if collection.entries.len() + 1 > sizes[index] as usize * 2 && index + 1 < sizes.len() {
        // The hash table's add / grow / resize. Rehash removes
        // ascending bucket heads into a reverse list, then prepends that list into new buckets:
        // entries which collide in the new table retain their previous iteration order.
        collection.bucket_index += 1;
        collection
            .entries
            .sort_by_key(|(key, _)| key % sizes[index + 1]);
    }
    let bucket_count = sizes[usize::from(collection.bucket_index)];
    let bucket = property.name % bucket_count;
    let at = collection
        .entries
        .iter()
        .position(|(key, _)| key % bucket_count >= bucket)
        .unwrap_or(collection.entries.len());
    collection.entries.insert(at, (property.name, property));
}

impl super::PlayerSystem {
    /// The retained local module's numeric window-property setter. The UI callers already guard
    /// window ID zero; retained-module absence is this host's ownership boundary before 0x0013.
    /// Same-value assignments still dirty the module, as the structure and hash-table setters do.
    /// The existing 480-second/explicit-save path owns serialization; this sends nothing.
    pub fn set_chat_window_option(
        &mut self,
        window: u32,
        property: u32,
        value: i32,
        now: ServerTime,
    ) -> bool {
        let value = match property {
            0x1000_0086..=0x1000_0089 => BasePropertyValue::Integer(value),
            0x1000_008A => BasePropertyValue::Bool(value != 0),
            _ => return false,
        };
        self.set_chat_window_property(window, property, value, now)
    }

    /// The title setter copies its `StringInfo` into property `0x1000008D` before calling the same
    /// retained player-module setter as numeric placement.
    pub fn set_chat_window_title(
        &mut self,
        window: u32,
        title: dereth_protocol::property::StringInfo,
        now: ServerTime,
    ) -> bool {
        self.set_chat_window_property(
            window,
            0x1000_008D,
            BasePropertyValue::StringInfo(title),
            now,
        )
    }

    /// Set chat-window option `0x1000007F` with a **`Bitfield64`** value — the
    /// Chat Options page's per-window text-type filter.
    ///
    /// The same walk as the placement setter; only the value
    /// type differs, and it has to, because `property_type(0x1000007F)` is `Bitfield64` and
    /// the property value setter's type check makes a wrong one a silent no-op.
    /// `ID_ChatOption_TextFilter_Society` is `0x100000000`, so an `i32` cannot carry it.
    ///
    /// **Sends nothing.** The setter's tail raises a local notice and sets the same deferred-save
    /// flag [`Self::use_time`]
    /// already drains.
    pub fn set_chat_window_filter(&mut self, window: u32, mask: u64, now: ServerTime) -> bool {
        self.set_chat_window_property(
            window,
            super::CHAT_TEXT_TYPE_FILTER,
            BasePropertyValue::Bitfield64(mask),
            now,
        )
    }

    /// Set a `Float` gameplay option from its base property —
    /// `0x10000080 Option_DefaultOpacity` / `0x10000081 Option_ActiveOpacity`.
    ///
    /// The client's named-property arm writes the **top level** of
    /// the gameplay-options collection, not the `0x1000008C` per-window array — the Chat Options page's
    /// general section is one pair of sliders for all five windows. Same dirty flag, same
    /// silence on the wire.
    pub fn set_gameplay_option_float(
        &mut self,
        property: u32,
        value: f32,
        now: ServerTime,
    ) -> bool {
        if dereth_protocol::property::property_type(property)
            != Some(dereth_protocol::property::BasePropertyType::Float)
        {
            return false;
        }
        let Some(module) = self.module.as_mut() else {
            return false;
        };
        let options = module.gameplay_options.get_or_insert_with(|| {
            dereth_protocol::property::PackObjPropertyCollection {
                properties: PropertyCollection::default(),
                ..Default::default()
            }
        });
        set(
            &mut options.properties,
            BaseProperty {
                name: property,
                value: Some(BasePropertyValue::Float(value)),
            },
        );
        self.mark_dirty(now);
        true
    }

    fn set_chat_window_property(
        &mut self,
        window: u32,
        property: u32,
        value: BasePropertyValue,
        now: ServerTime,
    ) -> bool {
        if window == 0 || window > MAX_WINDOWS {
            return false;
        }
        let Some(module) = self.module.as_mut() else {
            return false;
        };
        let options = module.gameplay_options.get_or_insert_with(|| {
            dereth_protocol::property::PackObjPropertyCollection {
                properties: PropertyCollection::default(),
                ..Default::default()
            }
        });
        if !options
            .properties
            .entries
            .iter()
            .any(|(key, _)| *key == ARRAY)
        {
            set(
                &mut options.properties,
                BaseProperty {
                    name: ARRAY,
                    value: Some(BasePropertyValue::Array(Vec::new())),
                },
            );
        }
        let array = &mut options
            .properties
            .entries
            .iter_mut()
            .find(|(key, _)| *key == ARRAY)
            .unwrap()
            .1;
        if dereth_protocol::property::property_type(array.name)
            != Some(dereth_protocol::property::BasePropertyType::Array)
        {
            return false;
        }
        let Some(BasePropertyValue::Array(rows)) = array.value.as_mut() else {
            return false;
        };
        let index = (window - 1) as usize;
        // The setter loads the existing count before filling through the requested index.
        rows.resize_with(rows.len().max(index + 1), empty_row);
        let Some(BasePropertyValue::Struct(fields)) = rows[index].value.as_mut() else {
            return false;
        };
        set(
            fields,
            BaseProperty {
                name: property,
                value: Some(value),
            },
        );
        // The player module's changed hook: only the first dirty time
        // is retained, including when the field write merely assigned the same value.
        self.mark_dirty(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::PlayerSystem;
    use dereth_protocol::login::PlayerModule;

    #[test]
    fn numeric_writes_build_dense_typed_rows_and_preserve_other_properties() {
        let mut p = PlayerSystem::new();
        p.apply_player_module(&PlayerModule::default());
        assert!(p.set_chat_window_option(2, 0x1000_0086, 77, ServerTime(10.0)));
        let original = p.module.clone().unwrap();
        assert!(p.set_chat_window_option(10, 0x1000_008A, 0, ServerTime(20.0)));
        let module = p.client_packed_module().unwrap();
        let Some(BasePropertyValue::Array(rows)) = module
            .gameplay_options
            .as_ref()
            .unwrap()
            .properties
            .get(ARRAY)
        else {
            panic!("placement array")
        };
        assert_eq!(rows.len(), 10);
        assert!(rows.iter().all(|r| r.name == ROW));
        let Some(BasePropertyValue::Array(before)) = original
            .gameplay_options
            .as_ref()
            .unwrap()
            .properties
            .get(ARRAY)
        else {
            panic!("old array")
        };
        assert_eq!(
            &rows[..2],
            before,
            "growth preserves existing windows instead of rebuilding them"
        );
        let Some(BasePropertyValue::Struct(fields)) = &rows[9].value else {
            panic!("row")
        };
        assert_eq!(
            fields.bucket_index, 0,
            "retail structured-property value default hash has 11 buckets"
        );
        assert_eq!(
            fields.get(0x1000_008A),
            Some(&BasePropertyValue::Bool(false))
        );
        let mut writer = dereth_protocol::Writer::new();
        module.write(&mut writer).unwrap();
        let bytes = writer.into_inner();
        let decoded = PlayerModule::read(&mut dereth_protocol::Reader::new(&bytes)).unwrap();
        assert_eq!(decoded.gameplay_options, module.gameplay_options);
        assert!(!p.use_time(ServerTime(489.99)));
        assert!(
            !p.use_time(ServerTime(490.0)),
            "retail flush comparison is strict"
        );
        assert!(
            p.use_time(ServerTime(490.01)),
            "first dirty time 10 is not replaced by 20"
        );
    }

    #[test]
    fn equal_assignment_still_dirties_but_absent_module_or_invalid_caller_does_not() {
        let mut p = PlayerSystem::new();
        assert!(!p.set_chat_window_option(10, 0x1000_0086, 1, ServerTime(1.0)));
        assert!(p.module.is_none());
        assert!(!p.is_dirty());
        p.apply_player_module(&PlayerModule::default());
        let original = p.module.clone();
        for window in [0, 0x1_0000, u32::MAX] {
            assert!(!p.set_chat_window_option(window, 0x1000_0086, 1, ServerTime(2.0)));
        }
        assert!(
            !p.set_chat_window_option(10, 0x1000_008D, 1234, ServerTime(2.0)),
            "StringInfo not inferred from a numeric payload"
        );
        assert_eq!(p.module, original);
        assert!(!p.is_dirty());
        assert!(p.set_chat_window_option(10, 0x1000_0086, 77, ServerTime(3.0)));
        let same = p.module.clone().unwrap();
        p.apply_player_module(&same);
        assert!(!p.is_dirty());
        assert!(p.set_chat_window_option(10, 0x1000_0086, 77, ServerTime(4.0)));
        assert_eq!(p.module.as_ref(), Some(&same));
        assert!(
            p.is_dirty(),
            "the hash-table setter returns true even for equal assignment"
        );
    }

    #[test]
    fn adding_a_property_grows_only_past_two_entries_per_bucket_and_preserves_values() {
        let mut fields = PropertyCollection::default();
        for name in 1..=22 {
            set(
                &mut fields,
                BaseProperty {
                    name,
                    value: Some(BasePropertyValue::Integer(name as i32)),
                },
            );
        }
        assert_eq!(fields.bucket_index, 0);
        let before = fields.clone();
        set(
            &mut fields,
            BaseProperty {
                name: 1,
                value: Some(BasePropertyValue::Integer(1)),
            },
        );
        assert_eq!(fields, before, "assignment is not insertion/growth");
        set(
            &mut fields,
            BaseProperty {
                name: 24,
                value: Some(BasePropertyValue::Integer(24)),
            },
        );
        assert_eq!(fields.bucket_index, 1);
        assert_eq!(
            fields
                .entries
                .iter()
                .map(|(key, _)| *key)
                .collect::<Vec<_>>(),
            std::iter::once(24).chain(1..=22).collect::<Vec<_>>(),
            "new key 24 prepends in bucket 1 of 23"
        );
        for (key, property) in before.entries {
            assert_eq!(
                fields.entries.iter().find(|(k, _)| *k == key).unwrap().1,
                property
            );
        }
    }
}
