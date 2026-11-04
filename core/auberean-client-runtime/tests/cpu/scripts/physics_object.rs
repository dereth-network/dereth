//! An object's physics object as the scripts reach it, each native called as the machine calls one.

use auberean_client_model::model::Place;
use auberean_client_runtime::body::{BodyMaterial, BodyReading};
use auberean_client_runtime::physics_object::BodyRequest;
use auberean_primitives::Vec3;
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::value::{tag, NULL};
use auberean_script::Status;

use crate::common::{Bench, ABSENT, ABSENT_REF, HOLDER, HOLDER_REF, ITEM, ITEM_REF};

const PHYSICS_OBJECT: u32 = 0x80;

/// A physics object's current properties are what the cells its body has been in made of it: an
/// object whose body has stood in none, or with no body, has none.
///
/// Behaviour: script.model.a-physics-objects-properties-are-its-bodys-material
#[test]
fn a_physics_objects_properties_are_its_bodys_material() {
    let mut b = Bench::new();
    b.host.materials.insert(ITEM, BodyMaterial::default());
    const COLLECTION: u32 = 0x5B;
    let out =
        b.vm.heap
            .add_native(COLLECTION, NativeObject::Text(Vec::new()));
    let args = [(out, tag::REFERENCE)];
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x4A, HOLDER_REF, &args), 0);
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x4A, ABSENT_REF, &args), 0);
    assert_eq!(
        b.vm.heap.native(out).unwrap(),
        &NativeObject::Text(Vec::new()),
        "nothing given"
    );
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x4A, ITEM_REF, &args), 1);
    assert!(matches!(
        b.vm.heap.native(out).unwrap(),
        NativeObject::Engine(_)
    ));
}

/// What the scripts read of a physics object comes from its body as a front end read it (the
/// radius and height of its top level from the object holding it); with no physics object, 0.
/// Whether it is in the world is its own; undetectable and frozen are as the scripts set them.
///
/// Behaviour: script.model.a-physics-object-reads-its-body
#[test]
fn a_physics_object_reads_its_body_and_none_reads_zero() {
    let mut b = Bench::new();
    b.host.readings.insert(
        ITEM,
        BodyReading {
            velocity_scale: 1.5,
            acceleration_scale: 0.5,
            jump_scale: 2.0,
            radius: 0.25,
            height: 1.75,
            mode: 0x4100_0003,
            desired_mode: 0x4100_0004,
            ..BodyReading::default()
        },
    );
    b.host.readings.insert(
        HOLDER,
        BodyReading {
            radius: 0.5,
            height: 2.0,
            ..BodyReading::default()
        },
    );
    let float = |v: u32| f32::from_bits(v);
    assert_eq!(float(b.answer(PHYSICS_OBJECT, 0x04, ITEM_REF, &[])), 1.5);
    assert_eq!(float(b.answer(PHYSICS_OBJECT, 0x06, ITEM_REF, &[])), 0.5);
    assert_eq!(float(b.answer(PHYSICS_OBJECT, 0x08, ITEM_REF, &[])), 2.0);
    assert_eq!(float(b.answer(PHYSICS_OBJECT, 0x4D, ITEM_REF, &[])), 0.25);
    assert_eq!(float(b.answer(PHYSICS_OBJECT, 0x4E, ITEM_REF, &[])), 1.75);
    assert_eq!(
        float(b.answer(PHYSICS_OBJECT, 0x4F, ITEM_REF, &[])),
        0.5,
        "the holder's"
    );
    assert_eq!(
        float(b.answer(PHYSICS_OBJECT, 0x50, ITEM_REF, &[])),
        2.0,
        "the holder's"
    );
    assert_eq!(
        float(b.answer(PHYSICS_OBJECT, 0x4F, HOLDER_REF, &[])),
        0.5,
        "its own"
    );
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x19, ITEM_REF, &[]), 0x4100_0003);
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x18, ITEM_REF, &[]), 0x4100_0004);
    assert_eq!(
        b.answer(PHYSICS_OBJECT, 0x63, ITEM_REF, &[]),
        0,
        "no motion asked"
    );
    b.host.readings.get_mut(&ITEM).unwrap().remote_motion = Some([0.0, 1.0, 0.0]);
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x63, ITEM_REF, &[]), 1);
    let v = b.answer(PHYSICS_OBJECT, 0x64, ITEM_REF, &[]);
    assert_eq!(
        b.vm.heap.native(v).unwrap(),
        &NativeObject::Vector([0, 1.0f32.to_bits(), 0])
    );
    // An object the client was never sent: 0 throughout, undetectable 1.
    b.host.readings.insert(ABSENT, b.host.readings[&ITEM]);
    for n in [
        0x04, 0x06, 0x08, 0x4D, 0x4E, 0x4F, 0x50, 0x18, 0x19, 0x63, 0x37,
    ] {
        assert_eq!(
            b.answer(PHYSICS_OBJECT, n, ABSENT_REF, &[]),
            0,
            "native {n:#x}"
        );
    }
    assert_eq!(
        b.answer(PHYSICS_OBJECT, 0x51, ABSENT_REF, &[]),
        1,
        "undetectable"
    );
    let v = b.answer(PHYSICS_OBJECT, 0x64, ABSENT_REF, &[]);
    assert_eq!(b.vm.heap.native(v).unwrap(), &NativeObject::Vector([0; 3]));
    // In the world once placed there; never while held.
    let place = Place {
        cell: 0x0101_0001,
        origin: Vec3::new(1.0, 2.0, 3.0),
        heading_motion: 0,
        position_stamp: 1,
        teleport_stamp: 1,
        force_position_stamp: 1,
        time: 0.0,
        flags: 0,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    };
    assert_eq!(
        b.answer(PHYSICS_OBJECT, 0x37, HOLDER_REF, &[]),
        0,
        "no place yet"
    );
    b.host.model.object_mut(HOLDER).unwrap().place = Some(place);
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x37, HOLDER_REF, &[]), 1);
    b.host.model.object_mut(ITEM).unwrap().place = Some(place);
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x37, ITEM_REF, &[]), 0, "held");
    // Undetectable and frozen as set.
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x51, ITEM_REF, &[]), 0);
    b.host.bodies.entry(ITEM).or_default().undetectable = Some(true);
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x51, ITEM_REF, &[]), 1);
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x53, ITEM_REF, &[]), 0);
    b.host.frozen.insert(ITEM, (true, false));
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x53, ITEM_REF, &[]), 1, "movement");
    assert_eq!(
        b.answer(PHYSICS_OBJECT, 0x55, ITEM_REF, &[]),
        0,
        "animation"
    );
    b.host.frozen.insert(ITEM, (false, true));
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x55, ITEM_REF, &[]), 1);
    // A null object crashes the call.
    assert_eq!(b.call(PHYSICS_OBJECT, 0x4E, NULL, &[], 1).0, Status::Crash);
}

/// What the scripts ask of a physics object goes, in order, to the body's requests; each answers
/// 1 (stopping a layer 0), and with no physics object 0 (stopping a layer 8) and nothing goes.
/// A scale answers nothing; a signalling not-a-number comes through quieted.
///
/// Behaviour: script.model.a-physics-object-passes-requests-to-its-body
#[test]
fn what_a_physics_object_is_asked_goes_to_its_body_in_order() {
    let mut b = Bench::new();
    let int = tag::INT;
    let (status, slots) = b.call(
        PHYSICS_OBJECT,
        0x03,
        ITEM_REF,
        &[(1.25f32.to_bits(), tag::FLOAT)],
        1,
    );
    assert_eq!((status, slots[0].0), (Status::Success, 0), "no answer");
    b.call(
        PHYSICS_OBJECT,
        0x05,
        ITEM_REF,
        &[(0x7F80_0001, tag::FLOAT)],
        0,
    );
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x58, ITEM_REF, &[(1, int)]), 1);
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x0C, ITEM_REF, &[(0, int)]), 1);
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x0D, ITEM_REF, &[(7, int)]), 1);
    // The id low word first.
    assert_eq!(
        b.answer(
            PHYSICS_OBJECT,
            0x2A,
            ITEM_REF,
            &[(0x8130_0000, int), (0x1000_0003, int)]
        ),
        1
    );
    let v = b.vm.heap.add_native(
        native_type::VECTOR,
        NativeObject::Vector([1.0f32.to_bits(), 0, 2.0f32.to_bits()]),
    );
    assert_eq!(
        b.answer(
            PHYSICS_OBJECT,
            0x24,
            ITEM_REF,
            &[(v, tag::REFERENCE), (3, int)]
        ),
        1
    );
    assert_eq!(
        b.answer(
            PHYSICS_OBJECT,
            0x25,
            ITEM_REF,
            &[(v, tag::REFERENCE), (0, int)]
        ),
        1
    );
    assert_eq!(
        b.answer(
            PHYSICS_OBJECT,
            0x26,
            ITEM_REF,
            &[
                (0x8130_0000, int),
                (0x1000_0003, int),
                (4.0f32.to_bits(), tag::FLOAT),
                (2, int),
                (0.5f32.to_bits(), tag::FLOAT),
            ],
        ),
        1
    );
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x28, ITEM_REF, &[(5, int)]), 0);
    let got = std::mem::take(&mut b.host.body_requests);
    let quiet = f32::from_bits(0x7FC0_0001);
    assert_eq!(got.len(), 10);
    assert_eq!(
        got[0],
        BodyRequest::VelocityScale {
            object: ITEM,
            scale: 1.25
        }
    );
    match got[1] {
        BodyRequest::AccelerationScale { object, scale } => {
            assert_eq!(object, ITEM);
            assert_eq!(scale.to_bits(), quiet.to_bits(), "quieted");
        }
        r => panic!("{r:?}"),
    }
    assert_eq!(
        got[2..],
        [
            BodyRequest::Cloaked {
                object: ITEM,
                on: true
            },
            BodyRequest::ReportCollisions {
                object: ITEM,
                on: false
            },
            BodyRequest::IgnoreCollisions {
                object: ITEM,
                on: true
            },
            BodyRequest::LookAt {
                object: ITEM,
                target: HOLDER
            },
            BodyRequest::Impulse {
                object: ITEM,
                vector: [1.0, 0.0, 2.0],
                local: false,
                flags: 3
            },
            BodyRequest::Impulse {
                object: ITEM,
                vector: [1.0, 0.0, 2.0],
                local: true,
                flags: 0
            },
            BodyRequest::TargetedImpulse {
                object: ITEM,
                target: HOLDER,
                speed: 4.0,
                flags: 2,
                extra: 0.5
            },
            BodyRequest::EndLayer {
                object: ITEM,
                layer: 5
            },
        ]
    );
    // With no physics object: nothing goes, the answers say so.
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x58, ABSENT_REF, &[(1, int)]), 0);
    assert_eq!(
        b.answer(PHYSICS_OBJECT, 0x2A, ABSENT_REF, &[(0, int), (0, int)]),
        0
    );
    assert_eq!(b.answer(PHYSICS_OBJECT, 0x28, ABSENT_REF, &[(5, int)]), 8);
    b.call(
        PHYSICS_OBJECT,
        0x03,
        ABSENT_REF,
        &[(1.0f32.to_bits(), tag::FLOAT)],
        0,
    );
    assert!(b.host.body_requests.is_empty());
    assert_eq!(
        b.call(PHYSICS_OBJECT, 0x58, NULL, &[(1, int)], 1).0,
        Status::Crash
    );
}
