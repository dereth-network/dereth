//! One message of every type, built in memory with every optional field present, encoded,
//! decoded and encoded again.
//! Behaviour: none (the codec agrees with itself; the recorded sessions are the claim)

use auberean_primitives::{Frame, Quat, Vec3};
use auberean_protocol::login::*;
use auberean_protocol::physics::*;
use auberean_protocol::quality::{PrivateQuality, VisualQuality};
use auberean_protocol::script::{ClientCall, FunctionId, ObjectCall, ScriptArgs, ServerCall};
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::text::{NarrowString, StringInfo, Variable, VariableValue, WideString};
use auberean_protocol::types::*;
use auberean_protocol::{ClientBlob, ClientMessage, ServerMessage};

fn stamp(id: u64) -> ObjectStamp {
    ObjectStamp {
        id,
        instance_stamp: 7,
        other_stamp: 9,
    }
}

fn place() -> Placement {
    Placement {
        cell: 0x7A69_0012,
        frame: Frame {
            origin: Vec3::new(1.5, 2.5, 3.5),
            rotation: Quat {
                w: 0.5,
                x: 0.5,
                y: 0.5,
                z: 0.5,
            },
        },
    }
}

fn info() -> StringInfo {
    let nested = StringInfo {
        string_id: 3,
        table_id: 4,
        ..StringInfo::default()
    };
    StringInfo {
        string_id: 0x10,
        table_id: 0x2300_0001,
        variable_count: 9,
        has_literal: 1,
        literal: Some(WideString::from("Dawnsong")),
        variables: vec![
            Variable {
                id: 1,
                kind: 2,
                value: VariableValue::Word(5),
            },
            Variable {
                id: 2,
                kind: 3,
                value: VariableValue::Words(6, 7, 8),
            },
            Variable {
                id: 3,
                kind: 4,
                value: VariableValue::Object(9, 10),
            },
            Variable {
                id: 4,
                kind: 5,
                value: VariableValue::ObjectWith(11, 12, 13, 14),
            },
            Variable {
                id: 5,
                kind: 9,
                value: VariableValue::Nested(Box::new(nested)),
            },
            Variable {
                id: 6,
                kind: 10,
                value: VariableValue::Long(15),
            },
            Variable {
                id: 7,
                kind: 11,
                value: VariableValue::LongWith(16, 17, 18),
            },
            Variable {
                id: 8,
                kind: 1,
                value: VariableValue::Empty,
            },
            Variable {
                id: 9,
                kind: 6,
                value: VariableValue::Word(19),
            },
        ],
    }
}

fn floats() -> Table<f32> {
    Table::from_inserted(SMALL, [(1, 0.25), (16, 1.0), (0x4100_001C, 0.75)])
}

fn behavior() -> BehaviorParams {
    BehaviorParams {
        bits: 0x00FF_FFFF,
        behavior: 1,
        time_scale: 1.25,
        mode: 2,
        hold_cycles: 3,
        fx_script: 4,
        fx: 5,
        target: 6,
        camera_target: 7,
        camera_behavior: 8,
        clone_visual: 9,
        clone_appearance: 10,
        clone_strengths: floats(),
        impulse: Vec3::new(1.0, 2.0, 3.0),
        words: [11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
        context: 24,
    }
}

fn part_group() -> PartGroupDesc {
    PartGroupDesc {
        bits: 0xFF,
        key: 0x4000_0002,
        parent_key: 2,
        connection_point: 3,
        setup: 4,
        anim_map: 5,
        appearance: Table::from_inserted(SMALL, [(0x2000_0009, floats()), (0x2000_0038, floats())]),
        fx_table: 6,
        startup_fx: floats(),
    }
}

fn visual() -> VisualDesc {
    VisualDesc {
        bits: 0x7FF,
        did: 0x1F00_0001,
        parent: 0x1F00_007A,
        motion_interp: 3,
        behavior_table: 4,
        modes: 5,
        scale: Vec3::new(0.95, 0.95, 1.01),
        child_scale: 1.5,
        particle_scale: 2.5,
        icon: vec![IconLayer(1, 2, 3)],
        global: part_group(),
        part_groups: Table::from_inserted(SMALL, [(7, part_group())]),
    }
}

fn physics() -> PhysicsDesc {
    PhysicsDesc {
        bits: 0x0FC2_9FBF,
        instance_stamp: 1,
        visual_ordering_stamp: 2,
        mode: 3,
        behaviors: vec![behavior()],
        sliders: vec![SliderSetting {
            slider: 1,
            value: 0.5,
            time: 2.0,
        }],
        anim_frame: 4,
        position: place(),
        parent: ParentLink {
            parent: 5,
            location: 6,
            stamp: 7,
        },
        orientation_id: 8,
        velocity: Vec3::new(1.0, 0.0, 0.0),
        external_acceleration: Vec3::new(0.0, 1.0, 0.0),
        velocity_scale: 1.1,
        jump_scale: 1.2,
        acceleration: Vec3::new(0.0, 0.0, 1.0),
        omega: Vec3::new(0.5, 0.0, 0.0),
        look_at: 9,
        head_x: 0.1,
        head_z: 0.2,
        target: 10,
        target_position: place(),
        target_offset: Vec3::new(1.0, 1.0, 1.0),
        fx: vec![RunningFx {
            fx: 1,
            scalar: 0.5,
            target: 11,
        }],
        stamps: [1, 2, 3, 4],
    }
}

fn weenie() -> WeenieDesc {
    WeenieDesc {
        mask: 0x7FFF_FFFF,
        package_id: 1,
        entity_did: 0x4700_110F,
        bitfield: 3,
        name: info(),
        plural_name: StringInfo::literal("Drudges"),
        icon: 4,
        objects: [1, 2, 3, 4, 5, 6, 7],
        scale: 1.5,
        numbers: [1, -2, 3, -4, 5, -6, 7, -8, 9, -10, 11, -12, 13],
    }
}

fn qualities() -> Qualities {
    Qualities {
        header: 0xFFF,
        did: 0x8100_0530,
        weenie: weenie(),
        ints: Table::from_inserted(SMALL, [(0x102, 5), (0x103, -6)]),
        bools: Table::from_inserted(SMALL, [(1, 1)]),
        floats: floats(),
        timestamps: Table::from_inserted(SMALL, [(1, 1.0e9)]),
        strings: Table::from_inserted(SMALL, [(1, NarrowString::from("motd"))]),
        data_ids: Table::from_inserted(SMALL, [(1, 0x4700_0001)]),
        instance_ids: Table::from_inserted(SMALL, [(1, 0x2130_0000_0000_0001)]),
        positions: Table::from_inserted(SMALL, [(1, place())]),
        string_infos: Table::from_inserted(SMALL, [(1, info())]),
        package_ids: Table::from_inserted(SMALL, [(1, 2)]),
        long_ints: Table::from_inserted(SMALL, [(1, 1 << 40)]),
    }
}

fn identity() -> CharacterIdentity {
    CharacterIdentity {
        id: 0x2130_0000_0000_1505,
        name: WideString::from("Babyfare"),
        seconds_greyed_out: 3600,
        visual: visual(),
    }
}

fn movement(to_object: bool) -> MovementParams {
    MovementParams {
        flags: if to_object { 0x325 } else { 0x305 },
        desired_velocity: 1.0,
        fail_distance: 2.0,
        desired_distance: 3.0,
        max_success_distance: 4.0,
        min_success_distance: 5.0,
        target: if to_object {
            MoveTarget::Object(77)
        } else {
            MoveTarget::Place(place())
        },
        offset: Vec3::new(1.0, 2.0, 3.0),
        offset_id: 6,
        waypoints: vec![place(), place()],
        context: 7,
    }
}

fn story() -> PhysicsStory {
    PhysicsStory {
        bits: 0x1E,
        attacker: 1,
        attacker_behavior: behavior(),
        attack_context: 2,
        skill: 3,
        hooks: vec![StoryHook {
            bits: 0x7FA3,
            target: 4,
            weapon: 5,
            numbers: [1, 2, 3, -4, -5, 6, 7],
        }],
    }
}

fn ev<A>(action: A) -> ObjectEvent<A> {
    ObjectEvent {
        sender: stamp(0x8130_0000_0000_0042),
        action,
    }
}

fn position(flags: u32) -> ObjectPosition {
    ObjectPosition {
        sender: stamp(1),
        time: 3.0e8,
        cell: 0x7A69_0012,
        origin: Vec3::new(1.0, 2.0, 3.0),
        heading_motion: 0x7F7F_7F7F,
        flags,
        position_stamp: 1,
        force_position_stamp: 2,
        teleport_stamp: 3,
        velocity: if flags & 0xA != 0 {
            Vec3::new(4.0, 5.0, 6.0)
        } else {
            Vec3::ZERO
        },
    }
}

fn server_messages() -> Vec<ServerMessage> {
    use ServerMessage as S;
    let args = Some(ScriptArgs(vec![1, 0, 0, 0, 2, 0, 0, 0]));
    vec![
        S::CharExitGame(Empty),
        S::CharacterDeleted(CharacterDeleted { character: 5 }),
        S::CharacterList(CharacterList {
            characters: vec![identity(), identity()],
            deleted: vec![identity()],
            status: 0,
            slots: 6,
            account: NarrowString::from("tester"),
            uses_chat_service: 1,
            has_expansion: 1,
            unknown: 1,
        }),
        S::CharacterRefused(CharacterRefused {
            reason: CharacterRefused::IN_WORLD,
        }),
        S::CharacterCreated(CharacterCreated {
            response: 1,
            identity: identity(),
            result: 0,
        }),
        S::CharacterNames(CharacterNames {
            number: 0,
            account: NarrowString::from("tester"),
            names: vec![WideString::from("A"), WideString::from("Bc")],
            ids: vec![1, 2],
        }),
        S::WorldName(Text(StringInfo::literal("Dawnsong"))),
        S::Notice(Notice {
            kind: 256,
            text: info(),
        }),
        S::AccountBooted(Text(info())),
        S::Chat(ChatPacket(vec![1, 2, 3, 4, 5])),
        S::DataRecord(DataRecord {
            dat_file: 0x2_0000_0001,
            kind: 0x4000_0050,
            id: 0x3F9E_FFFF,
            size: 0,
            bytes: vec![1, 2, 3],
        }),
        S::DataRefused(DataRefused {
            kind: 0x4000_0059,
            id: 0x2502_0001,
            error: 1,
        }),
        S::DataQuery(DataQuery {
            region: 1,
            name_language: 0x8000_0001,
            languages: vec![0x4100_0001, 0x8100_0001],
        }),
        S::DataPatchTimePending(Value(9)),
        S::DownloadBegins(DownloadBegins {
            bytes: 100,
            files: vec![MissingIterations {
                dat_file: 1 << 32,
                iteration: 5,
                download: vec![1, 2],
                purge: vec![3],
            }],
        }),
        S::DownloadEnds(Empty),
        S::PlayerObject(PlayerObject {
            id: 0x2130_0000_0000_0EF7,
            region: 1,
        }),
        S::PlayerQualities(PlayerQualities(qualities())),
        S::PlayerCall(ClientCall {
            function: FunctionId(0x8212_0027),
            args: args.clone(),
        }),
        S::PlayerCall(ClientCall {
            function: FunctionId(0x8212_0021),
            args: None,
        }),
        S::CellObjectCall(ObjectCall {
            object: stamp(3),
            call: ClientCall {
                function: FunctionId(0x820A_0009),
                args: args.clone(),
            },
        }),
        S::VisualObjectCall(ObjectCall {
            object: stamp(4),
            call: ClientCall {
                function: FunctionId(1),
                args,
            },
        }),
        S::PlayerCallAlternate(ClientCall {
            function: FunctionId(2),
            args: None,
        }),
        S::UpdateInt(PrivateQuality {
            quality: 0x103,
            value: -5,
        }),
        S::UpdateIntVisual(VisualQuality {
            object: stamp(2),
            quality: 0x102,
            value: 7,
        }),
        S::UpdateBool(PrivateQuality {
            quality: 1,
            value: 1,
        }),
        S::UpdateBoolVisual(VisualQuality {
            object: stamp(2),
            quality: 1,
            value: 0,
        }),
        S::UpdateFloat(PrivateQuality {
            quality: 1,
            value: 0.5,
        }),
        S::UpdateFloatVisual(VisualQuality {
            object: stamp(2),
            quality: 1,
            value: 0.5,
        }),
        S::UpdateString(PrivateQuality {
            quality: 1,
            value: NarrowString::from("abc"),
        }),
        S::UpdateStringVisual(VisualQuality {
            object: stamp(2),
            quality: 1,
            value: NarrowString::from("abcde"),
        }),
        S::UpdateDataId(PrivateQuality {
            quality: 1,
            value: 0x4700_0001,
        }),
        S::UpdateDataIdVisual(VisualQuality {
            object: stamp(2),
            quality: 1,
            value: 0x4700_0001,
        }),
        S::UpdateInstanceId(PrivateQuality {
            quality: 1,
            value: 1 << 50,
        }),
        S::UpdateInstanceIdVisual(VisualQuality {
            object: stamp(2),
            quality: 1,
            value: 1 << 50,
        }),
        S::UpdatePlacement(PrivateQuality {
            quality: 1,
            value: place(),
        }),
        S::UpdatePositionVisual(VisualQuality {
            object: stamp(2),
            quality: 1,
            value: place(),
        }),
        S::UpdateTimestamp(PrivateQuality {
            quality: 1,
            value: 2.5,
        }),
        S::UpdateTimestampVisual(VisualQuality {
            object: stamp(2),
            quality: 1,
            value: 2.5,
        }),
        S::UpdateStringInfo(PrivateQuality {
            quality: 1,
            value: info(),
        }),
        S::UpdateStringInfoVisual(VisualQuality {
            object: stamp(2),
            quality: 1,
            value: info(),
        }),
        S::UpdateLongInt(PrivateQuality {
            quality: 1,
            value: 1 << 40,
        }),
        S::UpdateLongIntVisual(VisualQuality {
            object: stamp(2),
            quality: 1,
            value: 1 << 40,
        }),
        S::CreateObject(CreateObject {
            id: 0x8130_0000_0000_0042,
            visual: visual(),
            physics: physics(),
            weenie: weenie(),
        }),
        S::PlayBehavior(ev(behavior())),
        S::StopBehavior(ev(Word(3))),
        S::MoveSlider(ev(Slide {
            slider: 1,
            value: 0.5,
            time: 1.0,
        })),
        S::DoMode(ev(Word(4))),
        S::MoveTo(ev(MoveOrder {
            params: movement(false),
            stamp: 9,
        })),
        S::MoveTo(ev(MoveOrder {
            params: movement(true),
            stamp: 10,
        })),
        S::Position(position(0x4)),
        S::Position(position(0x6)),
        S::Position(position(0xC)),
        S::Parent(ev(Attach {
            parent: stamp(5),
            location: 1,
            orientation: 2,
        })),
        S::HoverHeight(ev(Scale(1.5))),
        S::VelocityScale(ev(Scale(1.5))),
        S::AccelerationScale(ev(Scale(1.5))),
        S::MotionInterpChange(ev(Word(5))),
        S::EndLayer(ev(Word(6))),
        S::LookAt(ev(Target(77))),
        S::Push(ev(Impulse {
            direction: Vec3::new(1.0, 2.0, 3.0),
            contact: 1,
        })),
        S::PushToward(ev(TargetedImpulse {
            target: 1,
            magnitude: 2.0,
            falloff: 3,
            vertical: 4.0,
        })),
        S::LetGoBehavior(ev(Release {
            behavior: 1,
            params: behavior(),
        })),
        S::UpdateVisualDesc(ev(visual())),
        S::DoFx(ev(Fx { fx: 1, scalar: 0.5 })),
        S::StopFx(ev(Word(1))),
        S::DoTargetedFx(ev(TargetedFx {
            fx: 1,
            scalar: 0.5,
            target: 2,
        })),
        S::TerrainChange(ev(TerrainModifier {
            terrain: 1,
            modifier: 2,
        })),
        S::ClearTerrainChanges(ev(NoAction)),
        S::GlobalModifier(ev(Word(1))),
        S::DoStory(ev(story())),
        S::SetMode(ev(Word(2))),
        S::InitializeMissile(ev(Missile {
            position: place(),
            velocity: Vec3::new(1.0, 2.0, 3.0),
            target: 4,
            target_offset: place(),
            target_velocity: Vec3::new(5.0, 6.0, 7.0),
            initialized: 1,
        })),
        S::JumpScale(ev(Scale(2.0))),
        S::DoFxScript(ev(Word(3))),
        S::ObjectLeaves(ev(Leave { stamp: 4 })),
        S::PositionCell(position(0x4)),
        S::ForgetObject(ObjectOnly(stamp(9))),
        S::DeParent(ev(stamp(5))),
        S::StopAllBehaviors(ev(NoAction)),
        S::TurnHead(ev(HeadDirection { x: 0.5, y: -0.5 })),
        S::Contain(ObjectOnly(stamp(9))),
        S::StopMoveCancelsBehaviors(ev(NoAction)),
        S::PrivateFx(PrivateFx(vec![1, 2, 3])),
    ]
}

fn client_messages() -> Vec<ClientMessage> {
    use ClientMessage as C;
    vec![
        C::LogOff(CharacterRef {
            character: 0x2130_0000_0000_1505,
        }),
        C::DeleteCharacter(DeleteCharacter {
            account: NarrowString::from("tester"),
            character: 3,
        }),
        C::RestoreCharacter(RestoreCharacter {
            character: 3,
            name: WideString::from("Babyfare"),
            account: NarrowString::from("tester"),
        }),
        C::CreateCharacter(CreateCharacter {
            account: NarrowString::from("tester"),
            name: WideString::from("Babyfare"),
            entity_did: 0x4700_110F,
            start_area: 0x4000_0005,
            race: 4,
            sex: 0x2000,
            class: 4,
            implement: 8,
            physique: floats(),
        }),
        C::PlayCharacter(PlayCharacter {
            character: 0x2130_0000_0000_1505,
            account: NarrowString::from("tester"),
        }),
        C::DataRequest(DataRequest {
            kind: 0x4000_0026,
            id: 0x2502_16BB,
        }),
        C::DataHeld(DataHeld {
            language: 0x8000_0001,
            files: vec![DataFileIterations {
                dat_file: 1 << 32,
                iterations: IterationSet {
                    total: 137,
                    runs: vec![
                        IterationRun::Run {
                            count: 136,
                            first: 1,
                        },
                        IterationRun::One(140),
                    ],
                },
            }],
        }),
        C::DownloadEnds(Empty),
        C::ServerCall(ServerCall {
            function: FunctionId(0x820A_0007),
            args: ScriptArgs(vec![9; 36]),
        }),
        C::Position(ClientPosition {
            time: 3.0e8,
            cell: 0x7A69_0012,
            origin: Vec3::new(1.0, 2.0, 3.0),
            heading_motion: 1,
            flags: 0x6,
            instance_stamp: 1,
            teleport_stamp: 2,
            force_position_stamp: 3,
            move_to_stamp: 4,
            jump_velocity: Vec3::new(0.0, 0.0, 5.0),
        }),
        C::ForceCreate(Target(5)),
        C::LookAt(Target(6)),
        C::TurnHead(HeadDirection { x: 0.25, y: 0.75 }),
        C::SceneRenderingComplete(Empty),
        C::Chat(ChatPacket(vec![1, 2, 3])),
    ]
}

#[test]
fn every_server_message_type_encodes_and_decodes_to_itself() {
    let all = server_messages();
    for m in &all {
        let bytes = m.encode();
        assert_eq!(bytes[..4], m.message_type().to_le_bytes(), "{m:?}");
        let back = ServerMessage::decode(&bytes).unwrap_or_else(|e| panic!("{e}: {m:?}"));
        assert_eq!(&back, m);
        assert_eq!(back.encode(), bytes);
    }
    let mut types: Vec<u32> = all.iter().map(ServerMessage::message_type).collect();
    types.dedup();
    assert_eq!(types.len(), 80, "one sample per server type");
}

#[test]
fn every_client_message_type_encodes_and_decodes_to_itself() {
    for m in client_messages() {
        let bytes = m.encode();
        let back = ClientMessage::decode(&bytes).unwrap_or_else(|e| panic!("{e}: {m:?}"));
        assert_eq!(back, m);
        assert_eq!(
            ClientBlob::decode(4, &bytes).expect("plain queue"),
            ClientBlob::Message(m)
        );
    }
}

#[test]
fn a_secure_queue_payload_is_kept_whole_and_unread() {
    let bytes = vec![0xB6, 0x0D, 0x04, 0xC5, 1, 2, 3];
    let b = ClientBlob::decode(6, &bytes).expect("kept");
    assert!(matches!(b, ClientBlob::Secure { queue: 6, .. }));
    assert_eq!(b.encode(), bytes);
}

#[test]
fn a_truncated_or_padded_message_does_not_decode() {
    for m in server_messages() {
        let bytes = m.encode();
        if bytes.len() > 4 {
            assert!(
                ServerMessage::decode(&bytes[..bytes.len() - 1]).is_err(),
                "{m:?} short"
            );
        }
        let mut long = bytes.clone();
        long.extend_from_slice(&[0, 0, 0, 0]);
        if !matches!(
            m,
            ServerMessage::PlayerCall(_) | ServerMessage::PlayerCallAlternate(_)
        ) {
            assert!(ServerMessage::decode(&long).is_err(), "{m:?} long");
        }
    }
    assert!(ServerMessage::decode(&0x1234u32.to_le_bytes()).is_err());
}
