//! Shared virtual-time server fixture and message helpers.

#![allow(unused_imports)]
#![allow(clippy::disallowed_methods)]

/// Message decoding with synthetic bytes.
pub(crate) mod decode_table {
    pub(crate) use dereth_primitives::ObjectId;
    pub(crate) use dereth_primitives::{IncomingMessage, NetBlobId, NetQueue, RecipientId};
    pub(crate) use dereth_protocol::events::pack_event;
    pub(crate) use dereth_protocol::objects::{EffectsPlayerTeleport, ItemWearItem};
    pub(crate) use dereth_protocol::{self as proto, Message};
    pub(crate) use empyrean_testkit::decode;

    pub(crate) fn incoming(blob: &[u8]) -> IncomingMessage {
        IncomingMessage {
            opcode: u32::from_le_bytes(blob[0..4].try_into().expect("an opcode")),
            queue: NetQueue::UiQueue,
            sender: RecipientId(0),
            blob_id: NetBlobId(0),
            body: blob[4..].to_vec(),
        }
    }
}

#[cfg(feature = "real-content")]
pub(crate) mod real {
    pub(crate) use std::collections::BTreeMap;
    pub(crate) use std::sync::Arc;

    pub(crate) use dereth_primitives::NetQueue;
    pub(crate) use dereth_primitives::ObjectId;
    pub(crate) use dereth_protocol::combat::{
        AttackerNotification, CombatChangeCombatMode, CombatHandleAttackDoneEvent,
        CombatTargetedMeleeAttack, VictimNotificationSelf,
    };
    pub(crate) use dereth_protocol::comms::{CommunicationTalk, CommunicationTextboxString};
    pub(crate) use dereth_protocol::items::{
        InventoryGetAndWieldItem, InventoryPutItemInContainer, InventoryUseEvent,
    };
    pub(crate) use dereth_protocol::login::{
        CharGenResult, CharGenVerificationResponse, CharacterLoginCompleteNotification,
        CharacterSendCharGenResult, LoginCharacterSet, LoginExecuteLogOff,
        LoginExecuteLogOffRequest, LoginPlayerDescription, LoginSendEnterWorld,
        LoginSendEnterWorldRequest,
    };
    pub(crate) use dereth_protocol::movement::{
        AutonomousPosition, JumpPack, MoveTimestamps, MoveToStatePack, MovementAutonomousPosition,
        MovementJump, MovementMoveToState, MovementPositionEvent, MovementSetObjectMovement,
        MovementVectorUpdate, RawMotionState,
    };
    pub(crate) use dereth_protocol::objects::{
        EffectsPlayerTeleport, ItemCreateObject, ItemObjDescEvent, ItemOnViewContents,
        ItemParentEvent, ItemServerSaysContainId, ItemWearItem, LoginCreatePlayer,
    };
    pub(crate) use dereth_protocol::qualities::{
        MagicUpdateEnchantment, QualitiesPrivateUpdateAttribute2ndLevel,
    };
    pub(crate) use dereth_protocol::types::space::{Frame, PositionWire, Quat, Vec3};
    pub(crate) use dereth_protocol::Message;
    pub(crate) use empyrean_content::PackContent;
    pub(crate) use empyrean_dat::{DatManager, RealDats};
    pub(crate) use empyrean_entity::enums::{
        AccessLevel, CombatMode, EquipMask, PositionType, PropertyInt64, PropertyString,
    };
    pub(crate) use empyrean_entity::{ObjectGuid, Position};
    pub(crate) use empyrean_net::{SessionId, SessionState};
    pub(crate) use empyrean_testkit::{decode, ClientId, ClientStatus, TestServer};
    pub(crate) use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
    pub(crate) use empyrean_world::managers::player_manager;
    pub(crate) use empyrean_world::world_objects::entity::creature_attribute::StatCtx;
    pub(crate) use empyrean_world::world_objects::managers::enchantment_manager;
    pub(crate) use empyrean_world::world_objects::world_object::WorldObject;
    pub(crate) use empyrean_world::world_objects::{container, creature_equipment, monster_combat};

    pub(crate) const ACCOUNT: &str = "m1acct";
    /// A synthetic character name.
    pub(crate) const NAME: &str = "Mione Tester";

    /// The weenies the loop meets (ACE's world DB).
    pub(crate) const DRUDGE_SKULKER: u32 = 7;
    pub(crate) const BRUISED_APPLE: u32 = 5090;
    pub(crate) const TRAINING_DIRK: u32 = 12739;
    pub(crate) const HANDY_HEALING_KIT: u32 = 628;
    /// `SpellId.Vitae`.
    pub(crate) const VITAE: u32 = 666;

    /// The chargen spawn of start area 0 (Holtburg: the Training Academy).
    pub(crate) const ACADEMY_CELL: u32 = 0x8602_01AD;

    /// `MotionCommand.WalkForward` and `RunForward`, `HoldKey.None` and `HoldKey.Run`.
    pub(crate) const WALK_FORWARD: u32 = 0x4500_0005;
    pub(crate) const RUN_FORWARD: u32 = 0x4400_0007;
    pub(crate) const HOLD_NONE: u32 = 1;
    pub(crate) const HOLD_RUN: u32 = 2;
    pub(crate) const TRAINED: i32 = 2;

    pub(crate) const KNOWN_WIRE_DISAGREEMENTS: [(u32, &str); 2] = [
        (0x01B1, "AttackerNotification: 4 bytes left over"),
        (0x01B2, "DefenderNotification: 4 bytes left over"),
    ];

    pub(crate) fn dats() -> Arc<DatManager> {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        DatManager::initialize(Arc::new(source)).expect("the retail dats initialize")
    }

    pub(crate) fn pack() -> PackContent {
        let path = empyrean_common::test_paths::world_pack();
        PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        })
    }

    /// `GuidManager.Initialize` over the empty shard the server starts with.
    pub(crate) struct EmptyShard;

    impl ShardGuidQueries for EmptyShard {
        fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
            u32::MAX
        }
        fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
            Vec::new()
        }
    }

    // ---- the wire -----------------------------------------------------------------------------

    pub(crate) fn wire(p: &Position) -> PositionWire {
        PositionWire {
            objcell_id: p.cell(),
            frame: Frame {
                origin: Vec3 {
                    x: p.position_x,
                    y: p.position_y,
                    z: p.position_z,
                },
                orientation: Quat {
                    w: p.rotation_w,
                    x: p.rotation_x,
                    y: p.rotation_y,
                    z: p.rotation_z,
                },
            },
        }
    }

    pub(crate) fn move_to_state(
        at: &Position,
        raw_motion_state: RawMotionState,
    ) -> MovementMoveToState {
        MovementMoveToState(MoveToStatePack {
            raw_motion_state,
            position: wire(at),
            timestamps: MoveTimestamps::default(),
            contact: true,
            longjump_mode: false,
        })
    }

    /// The forward key held, walking (`HoldKey.None`) or running (`HoldKey.Run`).
    pub(crate) fn forward(at: &Position, hold: u32) -> MovementMoveToState {
        move_to_state(
            at,
            RawMotionState {
                current_holdkey: Some(hold),
                forward_command: Some(WALK_FORWARD),
                forward_holdkey: Some(hold),
                ..RawMotionState::default()
            },
        )
    }

    /// Every key released.
    pub(crate) fn stop(at: &Position) -> MovementMoveToState {
        move_to_state(
            at,
            RawMotionState {
                current_holdkey: Some(HOLD_NONE),
                ..RawMotionState::default()
            },
        )
    }

    pub(crate) fn autonomous(at: &Position) -> MovementAutonomousPosition {
        MovementAutonomousPosition(AutonomousPosition {
            position: wire(at),
            timestamps: MoveTimestamps::default(),
            contact: 1,
        })
    }

    /// An Aluvian male Soldier (template 6: Strength 100, Endurance 60, Coordination 100,
    /// Quickness 50, Focus 10, Self 10) with the requested skills trained, at start area 0.
    pub(crate) fn chargen(skills: &[usize]) -> CharacterSendCharGenResult {
        let mut sacs = vec![0; 55];
        for &skill in skills {
            sacs[skill] = TRAINED;
        }
        let mut result = CharGenResult {
            version: 1,
            heritage_group: 1,
            gender: 1,
            headgear_color: 3,
            shirt_color: 4,
            trousers_color: 5,
            footwear_color: 6,
            skin_shade: 1.0,
            hair_shade: 0.5,
            headgear_shade: 0.25,
            shirt_shade: 0.125,
            trousers_shade: 0.375,
            footwear_shade: 0.625,
            template_num: 6,
            strength: 100,
            endurance: 60,
            coordination: 100,
            quickness: 50,
            focus: 10,
            self_: 10,
            class_id: 1,
            skill_advancement_classes: sacs,
            name: NAME.to_owned(),
            start_area: 0,
            ..CharGenResult::default()
        };
        result.checksum_value = result.checksum();
        CharacterSendCharGenResult {
            account: ACCOUNT.to_owned(),
            result,
        }
    }

    // ---- the loop's state and helpers ---------------------------------------------------------

    /// The server, the bot, and what the loop has learned so far.
    pub(crate) struct Loop {
        pub(crate) ts: TestServer,
        pub(crate) id: ClientId,
        /// The character.
        pub(crate) g: ObjectGuid,
        /// Every `not_ported!` site hit so far, with counts.
        pub(crate) not_ported: BTreeMap<&'static str, u64>,
    }

    impl Loop {
        pub(crate) fn session(&self) -> SessionId {
            self.ts
                .world
                .net
                .find_by_account(ACCOUNT)
                .expect("the bot's session")
        }

        pub(crate) fn mark(&self) -> usize {
            self.ts.received_raw(self.id).len()
        }

        /// Every message of type `M` since `mark`, decoded (plain or game event).
        pub(crate) fn since<M: Message>(&self, mark: usize) -> Vec<M> {
            decode::all_of::<M>(&self.ts.received_raw(self.id)[mark..])
        }

        pub(crate) fn location_of(&self, o: ObjectGuid) -> Position {
            self.ts
                .world
                .objects
                .get(o)
                .and_then(WorldObject::location)
                .unwrap_or_else(|| panic!("{o:?} has a location"))
        }

        pub(crate) fn location(&self) -> Position {
            self.location_of(self.g)
        }

        pub(crate) fn health(&self, o: ObjectGuid) -> Option<u32> {
            self.ts.world.objects.get(o).map(|o| o.health().current(o))
        }

        pub(crate) fn action<M: Message>(&mut self, m: &M) {
            self.ts.send_game_action(self.id, m);
        }

        pub(crate) fn collect_not_ported(&mut self) {
            for (k, v) in TestServer::take_not_ported() {
                *self.not_ported.entry(k).or_insert(0) += v;
            }
        }

        pub(crate) fn advance(&mut self, secs: f64) {
            self.ts.advance(secs);
            self.collect_not_ported();
        }

        /// The objects in the character's landblock that `pred` accepts.
        pub(crate) fn nearby(&self, pred: impl Fn(&WorldObject) -> bool) -> Vec<ObjectGuid> {
            let Some(lb) = self
                .ts
                .world
                .objects
                .get(self.g)
                .and_then(|o| o.current_landblock)
            else {
                return Vec::new();
            };
            let Some(landblock) = self.ts.world.landblock_manager.landblocks.get(lb) else {
                return Vec::new();
            };
            landblock
                .get_all_world_objects_for_diagnostics()
                .into_iter()
                .filter(|&o| self.ts.world.objects.get(o).is_some_and(&pred))
                .collect()
        }

        pub(crate) fn nearby_wcid(&self, wcid: u32) -> Vec<ObjectGuid> {
            self.nearby(|o| o.biota.weenie_class_id == wcid)
        }

        /// What a client does after the server's MoveTo (or on its own): steps toward `target`,
        /// 0.5 m per 0.1 s, with AutonomousPositions, until it is within `within` metres of it
        /// (horizontally). The cell id is kept: every step stays in the Academy's entrance hall.
        pub(crate) fn walk_toward(&mut self, target: &Position, within: f32) {
            for _ in 0..400 {
                let mut at = self.location();
                let (dx, dy) = (
                    target.position_x - at.position_x,
                    target.position_y - at.position_y,
                );
                let d = dx.hypot(dy);
                if d <= within + 0.01 {
                    return;
                }
                let step = (d - within).min(0.5);
                at.position_x += dx / d * step;
                at.position_y += dy / d * step;
                self.action(&autonomous(&at));
                self.advance(0.1);
            }
            panic!(
                "never came within {within} m of {target:?}; at {:?}",
                self.location()
            );
        }

        /// Types `line` in chat as an admin: the session's access level and the character's
        /// `IsAdmin` for this one command, as an Admin account's login sets them
        /// (`Player.SetEphemeralValues` with `OverrideCharacterPermissions`), then back.
        pub(crate) fn admin_command(&mut self, line: &str) {
            let session = self.session();
            self.ts
                .world
                .sessions
                .get_mut(session)
                .expect("the game half")
                .access_level = AccessLevel::Admin;
            self.ts
                .world
                .objects
                .get_mut(self.g)
                .expect("in the world")
                .set_is_admin_prop(true);
            self.action(&CommunicationTalk {
                message: line.to_owned(),
            });
            self.advance(0.5);
            self.ts
                .world
                .sessions
                .get_mut(session)
                .expect("the game half")
                .access_level = AccessLevel::Player;
            self.ts
                .world
                .objects
                .get_mut(self.g)
                .expect("in the world")
                .set_is_admin_prop(false);
        }

        pub(crate) fn assert_all_decode(&self, mark: usize, step: &str) {
            let bad: Vec<_> = decode::undecoded(&self.ts.received_raw(self.id)[mark..])
                .into_iter()
                .filter(|d| {
                    !KNOWN_WIRE_DISAGREEMENTS.iter().any(|(k, e)| {
                        d.kind == *k && d.result.as_ref().err().map(String::as_str) == Some(*e)
                    })
                })
                .collect();
            assert!(
                bad.is_empty(),
                "{step}: messages dereth-protocol cannot decode: {:#?}",
                bad.iter()
                    .map(|d| (format!("0x{:04X} {}", d.kind, d.name()), d.result.clone()))
                    .collect::<Vec<_>>()
            );
        }

        /// The forward command (the wire's 16-bit command index) of the last UpdateMotion for the
        /// character since `mark`.
        pub(crate) fn last_forward(&self, mark: usize) -> Option<Option<u32>> {
            let m = self
                .since::<MovementSetObjectMovement>(mark)
                .into_iter()
                .filter(|m| m.id.0 == self.g.full())
                .last()?;
            let body = m
                .decoded_movement()
                .expect("the movement buffer decodes")
                .body;
            Some(
                body.interpreted
                    .and_then(|i| i.forward_command)
                    .map(u32::from),
            )
        }

        pub(crate) fn inventory_wcids(&self) -> Vec<u32> {
            container::inventory_values(&self.ts.world, self.g)
                .iter()
                .filter_map(|i| self.ts.world.objects.get(*i))
                .map(|o| o.biota.weenie_class_id)
                .collect()
        }

        pub(crate) fn total_xp(&self) -> Option<i64> {
            self.ts
                .world
                .objects
                .get(self.g)
                .and_then(|o| o.get_property(PropertyInt64::TotalExperience))
        }

        pub(crate) fn level(&self) -> Option<i32> {
            self.ts
                .world
                .objects
                .get(self.g)
                .and_then(WorldObject::level)
        }

        pub(crate) fn vitae(&self) -> Option<f32> {
            enchantment_manager::get_vitae(&self.ts.world, self.g).map(|v| v.stat_mod_value)
        }

        /// LoginSendEnterWorldRequest, LoginSendEnterWorld, then the client's LoginComplete.
        pub(crate) fn enter_world(&mut self) {
            self.ts
                .send_message(self.id, NetQueue::Logon, &LoginSendEnterWorldRequest);
            self.advance(0.1);
            self.ts.send_message(
                self.id,
                NetQueue::Logon,
                &LoginSendEnterWorld {
                    character: ObjectId(self.g.full()),
                    account: ACCOUNT.to_owned(),
                },
            );
            let g = self.g;
            assert!(
                self.ts.run_until(3.0, |ts| ts
                    .world
                    .objects
                    .get(g)
                    .is_some_and(|o| o.current_landblock.is_some())),
                "the character enters the world"
            );
            self.action(&CharacterLoginCompleteNotification);
            self.advance(1.0);
            let session = self.session();
            assert_eq!(
                self.ts.world.sessions.get(session).map(|s| s.state),
                Some(SessionState::WorldConnected)
            );
            assert!(
                player_manager::get_online_player(&self.ts.world, self.g.full()).is_some(),
                "online"
            );
        }
    }

    // ---- the steps ----------------------------------------------------------------------------

    /// Step 1: Connect, create a character through chargen, and enter the world at the chargen spawn.
    /// The February 2005 dat set (`DERETH_TEST_PRETOD_DAT_DIR`).
    pub(crate) fn pre_tod_dats() -> Arc<DatManager> {
        if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
            panic!("{msg}");
        }
        let dir = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
        let source = RealDats::open(&dir)
            .unwrap_or_else(|e| panic!("the February 2005 dats under {}: {e}", dir.display()));
        DatManager::initialize(Arc::new(source)).expect("the February 2005 dats initialize")
    }

    /// The Infiltration content pack (`EMPYREAN_TEST_INFILTRATION_PACK`).
    pub(crate) fn infiltration_pack() -> PackContent {
        let path = empyrean_common::test_paths::infiltration_pack();
        PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the February 2005 tests need an Infiltration world.pack at {} \
                 (EMPYREAN_TEST_INFILTRATION_PACK; `empyrean-import fetch --world 16py --pack \
                 --out <it>`): {e}",
                path.display()
            )
        })
    }

    pub(crate) fn create_character(skills: &[usize]) -> Loop {
        create_character_with(dats(), pack(), skills)
    }

    /// [`create_character`] on the given dats and content.
    pub(crate) fn create_character_with(
        dats: Arc<DatManager>,
        content: PackContent,
        skills: &[usize],
    ) -> Loop {
        let ts = TestServer::with_setup(dats, |w| {
            w.content = Arc::new(content);
            // The server refuses a pack built for another era than its own, so the world plays
            // the pack's.
            w.era = w.content.era().rules();
            guid_manager::initialize(w, &mut EmptyShard);
            w.auth
                .lock()
                .create_account(
                    ACCOUNT,
                    "pw",
                    AccessLevel::Player,
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                )
                .expect("created");
        });
        // Program.Main's CommandManager.Initialize (the `@` commands of step 4)
        empyrean_command::command_manager::initialize(None);
        let mut l = Loop {
            ts,
            id: ClientId(0),
            g: ObjectGuid::new(0),
            not_ported: BTreeMap::new(),
        };
        l.collect_not_ported();

        l.id = l.ts.connect(ACCOUNT, "pw");
        assert_eq!(
            l.ts.client(l.id).status(),
            ClientStatus::Connected,
            "logged in"
        );
        let id = l.id;
        assert!(
            l.ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()),
            "the character list arrives"
        );
        assert!(
            l.ts.received::<LoginCharacterSet>(id)[0]
                .characters
                .is_empty(),
            "a new account has no characters"
        );

        l.ts.send_message(id, NetQueue::Logon, &chargen(skills));
        assert!(
            l.ts.run_until(2.0, |ts| !ts
                .received::<CharGenVerificationResponse>(id)
                .is_empty()),
            "the chargen answer arrives"
        );
        let answer = l.ts.received::<CharGenVerificationResponse>(id);
        assert_eq!(
            answer.iter().map(|r| r.response_type).collect::<Vec<_>>(),
            [1],
            "CharacterGenerationVerificationResponse.Ok"
        );
        assert_eq!(answer[0].identity.name, NAME);
        l.g = ObjectGuid::new(answer[0].identity.gid.0);

        l
    }

    pub(crate) fn create_and_enter() -> Loop {
        let mut l = create_character(&[44, 6, 21, 22, 24]);
        let mark = l.mark();
        l.enter_world();
        assert_eq!(
            l.since::<LoginCreatePlayer>(mark).len(),
            1,
            "Login_CreatePlayer"
        );
        assert_eq!(
            l.since::<LoginPlayerDescription>(mark).len(),
            1,
            "the PlayerDescription"
        );
        let creates = l.since::<ItemCreateObject>(mark);
        assert!(
            creates.iter().any(|c| c.0.id.0 == l.g.full()),
            "the character's own CreateObject"
        );
        let known =
            empyrean_world::world_objects::player_tracking::get_known_objects(&l.ts.world, l.g)
                .into_iter()
                .filter(|&object| {
                    object != l.g
                        && l.ts
                            .world
                            .objects
                            .get(object)
                            .is_some_and(|o| !o.visibility())
                })
                .collect::<Vec<_>>();
        assert!(
            !known.is_empty(),
            "the player knows the Academy's nearby objects"
        );
        for object in known {
            assert!(
                creates.iter().any(|create| create.0.id.0 == object.full()),
                "known object {object:?} was created for the client"
            );
        }

        // PlayerFactory: Location and Sanctuary are the start area's first location
        let start = l.ts.world.dats.portal_dat().char_gen().starter_areas[0].locations[0];
        let at = l.location();
        assert_eq!((start.cell.0, at.cell()), (ACADEMY_CELL, ACADEMY_CELL));
        assert_eq!(
            (at.position_x, at.position_y),
            (start.frame.origin.x, start.frame.origin.y)
        );
        let sanctuary =
            l.ts.world
                .objects
                .get(l.g)
                .and_then(|o| o.get_position(PositionType::Sanctuary))
                .expect("a Sanctuary");
        assert_eq!(
            (sanctuary.cell(), sanctuary.position_x, sanctuary.position_y),
            (ACADEMY_CELL, at.position_x, at.position_y)
        );
        // starter gear: the Heavy Weapons dirk and the Healing kit, in the pack
        let pack = l.inventory_wcids();
        assert!(
            pack.contains(&TRAINING_DIRK) && pack.contains(&HANDY_HEALING_KIT),
            "{pack:?}"
        );
        l.assert_all_decode(0, "1. create and enter");
        l
    }

    /// Step 2: Walk, run, stop and jump.
    pub(crate) fn walk_run_jump(l: &mut Loop) {
        let start = l.location();

        // walk: MoveToState WalkForward (HoldKey.None) is broadcast as WalkForward; the
        // AutonomousPosition moves the character
        let mark = l.mark();
        l.action(&forward(&start, HOLD_NONE));
        l.advance(0.3);
        assert_eq!(
            l.last_forward(mark),
            Some(Some(WALK_FORWARD & 0xFFFF)),
            "UpdateMotion: walking"
        );
        let mut a = start;
        a.position_x -= 1.0;
        l.action(&autonomous(&a));
        l.advance(0.3);
        assert_eq!(
            (l.location().cell(), l.location().position_x),
            (ACADEMY_CELL, a.position_x),
            "walked 1 m"
        );

        // run: HoldKey.Run turns WalkForward into RunForward
        let mark = l.mark();
        l.action(&forward(&a, HOLD_RUN));
        l.advance(0.3);
        assert_eq!(
            l.last_forward(mark),
            Some(Some(RUN_FORWARD & 0xFFFF)),
            "UpdateMotion: running"
        );
        let mut b = a;
        b.position_x -= 1.5;
        l.action(&autonomous(&b));
        l.advance(0.3);
        assert_eq!(l.location().position_x, b.position_x, "ran 1.5 m");
        assert!(
            !l.since::<MovementPositionEvent>(mark).is_empty(),
            "UpdatePosition"
        );

        // stop
        let mark = l.mark();
        l.action(&stop(&b));
        l.advance(0.3);
        assert_eq!(l.last_forward(mark), Some(None), "UpdateMotion: standing");

        // jump: the stamina cost is paid and the jump is broadcast as a VectorUpdate
        let stamina = |l: &Loop| {
            l.ts.world
                .objects
                .get(l.g)
                .map(|o| o.stamina().current(o))
                .expect("stamina")
        };
        let before = stamina(l);
        let mark = l.mark();
        l.action(&MovementJump(JumpPack {
            extent: 0.5,
            velocity: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 5.0,
            },
            position: wire(&b),
            timestamps: MoveTimestamps::default(),
        }));
        l.advance(2.0);
        assert!(
            stamina(l) < before,
            "the jump costs stamina ({before} -> {})",
            stamina(l)
        );
        assert_eq!(
            l.since::<MovementVectorUpdate>(mark).len(),
            1,
            "the jump's VectorUpdate"
        );
        assert_eq!(l.location().cell(), ACADEMY_CELL, "landed in the hall");
        l.assert_all_decode(0, "2. walk, run, jump");
    }

    /// Step 3: Pick up the apple from its table, and wield the dirk from the pack.
    pub(crate) fn pick_up_and_wield(l: &mut Loop) {
        let apple = *l
            .nearby_wcid(BRUISED_APPLE)
            .first()
            .expect("the Academy's link generator spawned its bruised apple");
        let apple_at = l.location_of(apple);
        let mark = l.mark();
        l.action(&InventoryPutItemInContainer {
            item: ObjectId(apple.full()),
            container: ObjectId(l.g.full()),
            slot: 0,
        });
        l.advance(0.2);
        assert!(
            l.since::<MovementSetObjectMovement>(mark)
                .iter()
                .any(|m| m.id.0 == l.g.full()),
            "the server's MoveTo toward the apple"
        );
        l.walk_toward(&apple_at, 0.6);
        l.advance(2.0);
        let contain = l.since::<ItemServerSaysContainId>(mark);
        assert_eq!(
            contain
                .iter()
                .map(|c| (c.item.0, c.container.0))
                .collect::<Vec<_>>(),
            [(apple.full(), l.g.full())],
            "InventoryServerSaysContainId"
        );
        assert_eq!(
            l.ts.world
                .objects
                .get(apple)
                .and_then(WorldObject::container_id),
            Some(l.g.full()),
            "the apple is in the pack"
        );
        assert!(l.inventory_wcids().contains(&BRUISED_APPLE));

        let dirk = container::inventory_values(&l.ts.world, l.g)
            .into_iter()
            .find(|i| {
                l.ts.world
                    .objects
                    .get(*i)
                    .is_some_and(|o| o.biota.weenie_class_id == TRAINING_DIRK)
            })
            .expect("the dirk");
        let melee = EquipMask::MeleeWeapon.0;
        let mark = l.mark();
        l.action(&InventoryGetAndWieldItem {
            item: ObjectId(dirk.full()),
            slot: melee,
        });
        l.advance(1.0);
        assert_eq!(
            l.since::<ItemWearItem>(mark)
                .iter()
                .map(|w| (w.item.0, w.slot))
                .collect::<Vec<_>>(),
            [(dirk.full(), melee)],
            "WearItem"
        );
        // TryEquipObjectWithBroadcasting: the dirk goes to the hand (ParentEvent) and the new look
        // (ObjDescEvent) reaches the wielder too
        assert_eq!(l.since::<ItemParentEvent>(mark).len(), 1, "ParentEvent");
        assert!(
            l.since::<ItemObjDescEvent>(mark)
                .iter()
                .any(|e| e.id.0 == l.g.full()),
            "ObjDescEvent"
        );
        let d = l.ts.world.objects.get(dirk).expect("the dirk");
        assert_eq!(
            (d.wielder_id(), d.current_wielded_location()),
            (Some(l.g.full()), Some(EquipMask::MeleeWeapon))
        );
        l.assert_all_decode(0, "3. pick up and wield");
    }

    /// Step 4: Create a drudge, fight it until it dies, open its corpse and take an item. Returns the
    /// looted item.
    pub(crate) fn kill_and_loot_a_drudge(l: &mut Loop) -> ObjectGuid {
        let drudge = create_a_drudge(l);
        let mark = l.mark();
        fight(l, drudge);
        assert!(
            !l.since::<AttackerNotification>(mark).is_empty(),
            "the character's hits land (AttackerNotification)"
        );
        loot(l)
    }

    /// `@create drudgeskulker`: one drudge skulker in front of the character.
    pub(crate) fn create_a_drudge(l: &mut Loop) -> ObjectGuid {
        let mark = l.mark();
        l.admin_command("@create drudgeskulker");
        let drudges = l.nearby_wcid(DRUDGE_SKULKER);
        assert_eq!(drudges.len(), 1, "@create made one drudge skulker");
        let drudge = drudges[0];
        assert!(
            l.since::<ItemCreateObject>(mark)
                .iter()
                .any(|c| c.0.id.0 == drudge.full()),
            "the client is told about the drudge"
        );
        drudge
    }

    /// Melee (power 0.5, medium height) until the drudge dies; the character survives.
    pub(crate) fn fight(l: &mut Loop, drudge: ObjectGuid) {
        let mark = l.mark();
        l.action(&CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Melee.0).expect("a mode"),
        });
        l.advance(1.0);
        let fight_start = l.ts.seconds();
        let mut attacks = 0;
        let mut dones = 0;
        while l
            .ts
            .world
            .objects
            .get(drudge)
            .is_some_and(|o| !monster_combat::is_dead(o))
            && l.health(l.g).unwrap_or(0) > 0
        {
            assert!(
                l.ts.seconds() - fight_start < 120.0,
                "the fight ends within two minutes"
            );
            let at = l.location_of(drudge);
            l.walk_toward(&at, 1.0);
            // the client attacks again after each AttackDone (no auto-repeat)
            let done = l.since::<CombatHandleAttackDoneEvent>(mark).len();
            if attacks == 0 || done > dones {
                dones = done;
                l.action(&CombatTargetedMeleeAttack {
                    target: ObjectId(drudge.full()),
                    attack_height: 2,
                    power_level: 0.5,
                });
                attacks += 1;
            }
            l.advance(0.25);
        }
        l.advance(2.0);
        assert!(
            l.health(l.g).unwrap_or(0) > 0,
            "the character survives its first drudge"
        );
        assert!(
            l.ts.world.objects.get(drudge).is_none(),
            "the drudge is dead and gone"
        );
        assert!(l.total_xp().unwrap_or(0) > 0, "the kill granted XP");
    }

    /// Opens the drudge's corpse with Use and takes its first item; returns the item.
    pub(crate) fn loot(l: &mut Loop) -> ObjectGuid {
        let corpses = l.nearby(|o| {
            o.is_corpse()
                && o.get_property(PropertyString::Name).as_deref()
                    == Some("Corpse of Drudge Skulker")
        });
        assert_eq!(corpses.len(), 1, "the drudge left a corpse");
        let corpse = corpses[0];
        let loot = container::inventory_values(&l.ts.world, corpse);
        assert!(!loot.is_empty(), "the corpse holds the drudge's treasure");

        // Use: the MoveTo, then Corpse.Open's ViewContents
        let corpse_at = l.location_of(corpse);
        let mark = l.mark();
        l.action(&InventoryUseEvent {
            object: ObjectId(corpse.full()),
        });
        l.advance(0.2);
        l.walk_toward(&corpse_at, 1.0);
        l.advance(1.0);
        let views = l.since::<ItemOnViewContents>(mark);
        assert_eq!(views.len(), 1, "ViewContents");
        assert_eq!(views[0].container.0, corpse.full());
        // (in PlacementPosition order; the set is what matters here)
        let mut listed: Vec<u32> = views[0].contents.iter().map(|c| c.iid.0).collect();
        let mut held: Vec<u32> = loot.iter().map(|g| g.full()).collect();
        listed.sort_unstable();
        held.sort_unstable();
        assert_eq!(listed, held, "it lists the loot");

        // take the first item into the pack
        let item = loot[0];
        let mark = l.mark();
        l.action(&InventoryPutItemInContainer {
            item: ObjectId(item.full()),
            container: ObjectId(l.g.full()),
            slot: 0,
        });
        l.advance(0.3);
        l.walk_toward(&corpse_at, 0.3);
        l.advance(2.0);
        assert!(
            l.since::<ItemServerSaysContainId>(mark)
                .iter()
                .any(|c| (c.item.0, c.container.0) == (item.full(), l.g.full())),
            "the item moves to the pack"
        );
        assert_eq!(
            l.ts.world
                .objects
                .get(item)
                .and_then(WorldObject::container_id),
            Some(l.g.full())
        );
        assert!(!container::inventory_values(&l.ts.world, corpse).contains(&item));
        l.assert_all_decode(0, "4. kill and loot a drudge");
        item
    }

    /// Step 5: A second drudge kills the character at peace: the death, the corpse, the vitae, the
    /// teleport to the sanctuary and the respawn vitals.
    pub(crate) fn die_and_respawn(l: &mut Loop, create_spot: &Position) {
        l.action(&CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::NonCombat.0).expect("a mode"),
        });
        l.advance(1.0);
        // back where the first @create placed its drudge: from beside the corpse, the spot 2 m
        // ahead is in another indoor cell and the create fails (PositionExtensions.GetIndoorCell
        // is a pointer; see the ignored test below)
        l.walk_toward(create_spot, 0.0);
        l.admin_command("@create drudgeskulker");
        assert_eq!(l.nearby_wcid(DRUDGE_SKULKER).len(), 1, "a second drudge");

        let level = l.level().expect("a level");
        let xp = l.total_xp();
        let death_spot = l.location();
        let mark = l.mark();
        let (g, id) = (l.g, l.id);
        assert!(
            l.ts.run_until(300.0, |ts| !decode::all_of::<EffectsPlayerTeleport>(
                &ts.received_raw(id)[mark..]
            )
            .is_empty()),
            "the drudge kills the character and it is teleported away"
        );
        l.collect_not_ported();
        let victim = l.since::<VictimNotificationSelf>(mark);
        assert_eq!(victim.len(), 1, "the death message (VictimNotification)");
        assert!(
            victim[0].message.contains("Drudge Skulker"),
            "{:?}",
            victim[0].message
        );
        assert!(
            l.since::<CommunicationTextboxString>(mark)
                .iter()
                .any(|m| m.text
                    == "You have retained all your items. You do not need to recover your corpse!"),
            "a level-{level} character keeps its items"
        );
        // Player.InflictVitaePenalty: 5% off, but no lower than GetMinVitae(level), whose largest
        // penalty is (level - 1) * 3% (at least 1%)
        let enchantments = l.since::<MagicUpdateEnchantment>(mark);
        let vitae = enchantments
            .iter()
            .find(|e| e.0.id & 0xFFFF == VITAE)
            .expect("the Vitae enchantment");
        let max_penalty = u16::try_from((level - 1) * 3)
            .expect("a low level")
            .clamp(1, 5);
        let expected_vitae = f32::from(100 - max_penalty) / 100.0;
        assert!(
            (vitae.0.smod.value - expected_vitae).abs() < 1e-6,
            "vitae {} for level {level}",
            vitae.0.smod.value
        );
        assert_eq!(l.vitae(), Some(vitae.0.smod.value));
        assert!(
            l.total_xp().unwrap_or(0) >= xp.unwrap_or(0),
            "death costs no XP: {:?} -> {:?}",
            xp,
            l.total_xp()
        );
        let name = format!("Corpse of {NAME}");
        let corpse = l.nearby(|o| {
            o.is_corpse() && o.get_property(PropertyString::Name).as_deref() == Some(name.as_str())
        });
        assert_eq!(corpse.len(), 1, "the character's corpse");
        let corpse_at = l.location_of(corpse[0]);
        assert!(
            (corpse_at.position_x - death_spot.position_x)
                .hypot(corpse_at.position_y - death_spot.position_y)
                < 1.0,
            "where it died"
        );

        // the client leaves portal space; 3 s after arrival the vitals are 75% of max
        l.advance(1.0);
        l.action(&CharacterLoginCompleteNotification);
        assert!(
            l.ts.run_until(10.0, |ts| ts
                .world
                .objects
                .get(g)
                .is_some_and(|o| o.health().current(o) > 0)),
            "the character is revived"
        );
        l.advance(0.5);
        let vital = l.ts.world.objects.get(g).expect("in the world").health();
        let max = vital.max_value(&mut StatCtx::in_world(&mut l.ts.world, g));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let expected = (f64::from(max as f32 * 0.75)).round() as u32; // (uint)Math.Round(Health.MaxValue * 0.75f)
        assert_eq!(l.health(g), Some(expected), "75% of max health ({max})");
        let levels = decode::all_of::<QualitiesPrivateUpdateAttribute2ndLevel>(
            &l.ts.received_raw(id)[mark..],
        );
        assert!(
            levels.iter().any(|u| u.0.value == expected),
            "the client is told the new health"
        );
        let at = l.location();
        let sanctuary =
            l.ts.world
                .objects
                .get(g)
                .and_then(|o| o.get_position(PositionType::Sanctuary))
                .expect("a Sanctuary");
        assert_eq!(
            (at.cell(), at.position_x, at.position_y),
            (sanctuary.cell(), sanctuary.position_x, sanctuary.position_y),
            "respawned at the sanctuary"
        );
        l.assert_all_decode(0, "5. die and respawn");
    }

    /// Step 6: Log off, log back on: the position, the pack, the wielded dirk, the vitae and the XP
    /// persisted.
    pub(crate) fn relog(l: &mut Loop, looted: ObjectGuid) {
        let before_at = l.location();
        let xp = l.total_xp();
        let level = l.level();
        let vitae = l.vitae();
        let mut pack = l.inventory_wcids();
        pack.sort_unstable();
        let looted_wcid =
            l.ts.world
                .objects
                .get(looted)
                .map(|o| o.biota.weenie_class_id)
                .expect("the looted item");

        let mark = l.mark();
        l.ts.send_message(
            l.id,
            NetQueue::Logon,
            &LoginExecuteLogOffRequest {
                character: ObjectId(l.g.full()),
            },
        );
        let (g, id) = (l.g, l.id);
        assert!(
            l.ts.run_until(10.0, |ts| player_manager::get_online_player(
                &ts.world,
                g.full()
            )
            .is_none()),
            "offline"
        );
        assert!(
            l.ts.run_until(10.0, |ts| !decode::all_of::<LoginExecuteLogOff>(
                &ts.received_raw(id)[mark..]
            )
            .is_empty()),
            "back at character select"
        );
        l.advance(0.5);
        assert!(
            l.ts.world.objects.get(g).is_none(),
            "released from the world"
        );
        let saved =
            l.ts.shard()
                .get_biota(g.full(), false)
                .expect("the character is in the shard");
        let saved =
            empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(
                &saved, false,
            );
        assert_eq!(
            saved.get_property(PropertyInt64::TotalExperience),
            xp,
            "XP saved"
        );

        l.enter_world();
        let at = l.location();
        assert_eq!(
            (at.cell(), at.position_x, at.position_y),
            (before_at.cell(), before_at.position_x, before_at.position_y),
            "at the sanctuary spot"
        );
        assert_eq!(l.total_xp(), xp, "XP");
        assert_eq!(l.level(), level, "level");
        assert_eq!(l.vitae(), vitae, "vitae");
        let mut back = l.inventory_wcids();
        back.sort_unstable();
        assert_eq!(back, pack, "the pack");
        assert!(
            back.contains(&looted_wcid) && back.contains(&BRUISED_APPLE),
            "the looted item and the apple"
        );
        let wielded = creature_equipment::equipped_objects_values(&l.ts.world, l.g);
        assert!(
            wielded.iter().any(|w| l
                .ts
                .world
                .objects
                .get(*w)
                .is_some_and(|o| o.biota.weenie_class_id == TRAINING_DIRK)),
            "the dirk is still wielded"
        );
        l.assert_all_decode(0, "6. relog");
    }

    // ---- the gaps: each is a step of the loop that cannot pass yet --------
}
