//! Decoders over records built in memory: what exact consumption means, how properties are read by
//! their declared type, and how a type with no layout yet reports itself.
//! Behaviour: none (decoder mechanics on synthetic records)

use auberean_assets::picture::Picture;
use auberean_assets::property::{
    PropertyCollection, PropertyDescriptor, PropertyList, PropertyValue,
};
use auberean_assets::{decode, Context, Record};
use auberean_dat::{Reader, RecordType};
use auberean_primitives::scramble;

fn words(ws: &[u32]) -> Vec<u8> {
    ws.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn picture_bytes() -> Vec<u8> {
    let mut b = words(&[0x4100_0001, 7, 2, 1, 0x15, 8]);
    b.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    b
}

#[test]
fn a_picture_decodes_to_its_size_format_and_bytes() {
    let p: Picture = decode(0x4100_0001, &picture_bytes(), &Context::empty()).unwrap();
    assert_eq!(
        (p.category, p.width, p.height, p.format_name().as_str()),
        (7, 2, 1, "A8R8G8B8")
    );
    assert_eq!(p.bytes, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn a_record_with_a_byte_left_over_does_not_decode() {
    let mut b = picture_bytes();
    b.push(0);
    let e = decode::<Picture>(0x4100_0001, &b, &Context::empty()).unwrap_err();
    assert!(e.what.contains("left over"), "{e}");
}

#[test]
fn a_record_filed_under_another_id_does_not_decode() {
    assert!(decode::<Picture>(0x4100_0002, &picture_bytes(), &Context::empty()).is_err());
}

fn context_with(kinds: &[(u32, u32)]) -> Context {
    Context::with_properties(PropertyList::from_descriptors(kinds.iter().map(
        |(id, kind)| {
            (
                *id,
                PropertyDescriptor {
                    value_kind: *kind,
                    group: 0,
                    data: 0,
                    report: 0,
                    required: false,
                },
            )
        },
    )))
}

#[test]
fn a_property_value_is_read_by_the_type_its_list_declares() {
    let ctx = context_with(&[
        (0x10, 0x4000_0003),
        (0x11, 0x4000_0006),
        (0x12, 0x4000_000D),
    ]);
    let name = scramble::conceal(b"oak");
    let mut b = words(&[1, 0x99, 3, 0x10]);
    b.extend_from_slice(&1.5f32.to_le_bytes());
    b.extend_from_slice(&words(&[0x11]));
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&name);
    b.extend_from_slice(&[0, 0, 0]);
    b.extend_from_slice(&words(&[0x12]));
    b.extend_from_slice(&(-2i64).to_le_bytes());
    let mut r = Reader::new(&b);
    let c = PropertyCollection::read(&mut r, &ctx).unwrap();
    r.finish().unwrap();
    let values: Vec<&PropertyValue> = c.iter().map(|p| &p.value).collect();
    assert_eq!(
        values,
        vec![
            &PropertyValue::Float(1.5),
            &PropertyValue::String("oak".into()),
            &PropertyValue::LongInteger(-2)
        ]
    );
    assert_eq!(c.groups[0].group, 0x99);
}

#[test]
fn a_property_the_list_does_not_declare_does_not_decode() {
    let ctx = context_with(&[(0x10, 0x4000_0002)]);
    let b = words(&[1, 0, 1, 0x20, 5]);
    let e = PropertyCollection::read(&mut Reader::new(&b), &ctx).unwrap_err();
    assert!(e.what.contains("not in the property list"), "{e}");
}

#[test]
fn a_type_without_a_layout_yet_says_so() {
    let r = Record::decode(
        RecordType::PATH_MAP,
        0x5D3A_FFFD,
        &words(&[0x5D3A_FFFD]),
        &Context::empty(),
    );
    assert_eq!(r.unwrap().unwrap_err().what, "not decoded yet");
    assert!(Record::decode(RecordType::LIGHT_CACHE, 0, &[], &Context::empty()).is_none());
}

#[test]
fn a_name_s_token_is_the_one_its_string_table_keys_it_by() {
    use auberean_assets::strings::token;
    assert_eq!(token("ID_Hello"), 0x2824_938C);
    assert_eq!(token("ID_Description"), 0x6C45_9364);
    assert_eq!(token(""), 0);
    assert_ne!(token("ID_Begin1"), token("ID_Begin2"));
}

#[test]
fn a_map_of_pairs_reads_back_and_a_short_one_does_not() {
    use auberean_assets::tables::{PairEntry, PairMap};
    let d = PairMap {
        map: vec![
            PairEntry { key: 1, value: 2 },
            PairEntry { key: 3, value: 4 },
        ],
    };
    let mut w = auberean_dat::Writer::new();
    d.write(&mut w);
    let bytes = w.finish();
    assert_eq!(bytes, words(&[2, 1, 2, 3, 4]));
    assert_eq!(PairMap::read(&mut Reader::new(&bytes)).unwrap(), d);
    assert!(PairMap::read(&mut Reader::new(&words(&[3, 1, 2]))).is_err());
    assert!(PairMap::read(&mut Reader::new(&[1, 0])).is_err());
}

#[test]
fn a_qualities_records_description_names_its_icon_and_its_names_tables() {
    use auberean_assets::property::StringInfo;
    use auberean_assets::script::ObjectDescription;
    use auberean_dat::QualifiedIds;
    use auberean_primitives::DataId;
    let named = |table: u32| StringInfo {
        table: DataId(table),
        ..StringInfo::default()
    };
    let d = ObjectDescription {
        icon: Some(DataId(0x0600_1234)),
        name: Some(named(0x2300_0001)),
        plural_name: Some(named(0x2300_0002)),
        ..ObjectDescription::default()
    };
    let mut out = QualifiedIds::new();
    d.sub_ids(&mut out);
    let ids: Vec<u32> = out.iter().map(|q| q.id).collect();
    assert_eq!(ids.len(), 3, "{ids:x?}");
    for id in [0x0600_1234, 0x2300_0001, 0x2300_0002] {
        assert!(ids.contains(&id), "{id:x} in {ids:x?}");
    }
    let mut none = QualifiedIds::new();
    ObjectDescription::default().sub_ids(&mut none);
    assert_eq!(none.iter().count(), 0, "no icon and no names name nothing");
}

/// A new sound setting has no flags, no random pitch (each end -1) and fades from a distance of 1,
/// and a new music setting fades from 1 too; a setting tests any of a mask's flags, and needs its
/// wave or music.
///
/// Behaviour: data.sound-info.new-settings-and-what-they-need
#[test]
fn new_sound_and_music_settings_and_what_they_need() {
    use auberean_assets::database::gather_sub_ids;
    use auberean_assets::sound::{MusicInfo, SoundInfo};
    use auberean_dat::QualifiedIds;
    let s = SoundInfo::default();
    assert_eq!(
        (
            s.flags,
            s.pitch_high,
            s.pitch_low,
            s.priority,
            s.min_distance,
            s.sound
        ),
        (0, -1.0, -1.0, 0.0, 1.0, 0)
    );
    let m = MusicInfo::default();
    assert_eq!(
        (m.flags, m.priority, m.min_distance, m.kind, m.music),
        (0, 0.0, 1.0, 0, 0)
    );
    let s = SoundInfo {
        flags: 0b0110,
        sound: 0x0C00_0001,
        ..s
    };
    let m = MusicInfo {
        flags: 0b0001,
        music: 0x2400_0002,
        ..m
    };
    assert!(s.test_bit_mask(0b0011) && !s.test_bit_mask(0b1001));
    assert!(m.test_bit_mask(0b0011) && !m.test_bit_mask(0b0010));
    let mut out = QualifiedIds::new();
    gather_sub_ids(&s, 0, &mut out);
    gather_sub_ids(&m, 0, &mut out);
    let ids: Vec<u32> = out.iter().map(|q| q.id).collect();
    assert_eq!(ids, [0x0C00_0001, 0x2400_0002]);
}
