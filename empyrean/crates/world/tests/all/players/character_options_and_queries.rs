//! ACE: Source/ACE.Server/Network/GameAction/Actions/GameActionSetCharacterOptions.cs::Handle
//! Tests of character options and queries.
//! Fixture: isolated world state and the shared area fixtures.

use crate::social::fellowship;

mod requests {
    use crate::support::player_world::*;

    /// `GameActionPingRequest`: answered with `GameEventPingResponse` (0x01EA).
    #[test]
    fn a_ping_request_is_answered() {
        let mut h = super::fellowship::H::small();
        let sa = h.player(A, "Alpha", 3);
        in_world(&mut h, sa);
        start_capture();
        action(&mut h.w, sa, &CharacterRequestPing);
        assert_eq!(events_to(&sent(), sa), [0x01EA]);
    }

    /// `GameActionQueryAge`: `GameEventQueryAgeResponse` (0x01C3) with an empty target name and
    /// `CalculateAgeMessage(Age ?? 0)`.
    ///
    /// Not ACE's (V301, owner 2026-09-24, retail): the target is read as the u32 object id the client
    /// sends, so a non-self target is answered too, always with the requester's age (V301, owner
    /// 2026-09-24). ACE read a String16L, which threw on an id whose low word is above 2 (0x5000_0103
    /// here).
    #[test]
    fn query_age_answers_with_the_age() {
        for target in [0, 0x5000_0002, 0x5000_0103] {
            let mut h = super::fellowship::H::small();
            let sa = h.player(A, "Alpha", 3);
            in_world(&mut h, sa);
            h.w.objects
                .get_mut(A)
                .unwrap()
                .set_property(PropertyInt::Age, 90_061);
            start_capture();
            action(
                &mut h.w,
                sa,
                &CharacterQueryAge {
                    target: ObjectId(target),
                },
            );
            let msgs = sent();
            assert_eq!(events_to(&msgs, sa), [0x01C3]);
            let body = &msgs.iter().find(|m| m.0 == sa && m.2 == 0x01C3).unwrap().3;
            // (header 16 bytes) target name "" (2 + pad 2), then "1d 1h 1m 1s"
            assert_eq!(&body[16..20], &[0, 0, 0, 0]);
            let len = usize::from(u16::from_le_bytes([body[20], body[21]]));
            assert_eq!(
                std::str::from_utf8(&body[22..22 + len]).unwrap(),
                "1d 1h 1m 1s",
                "target {target:#X}"
            );
        }
    }

    /// `GameActionQueryBirth`: a system chat with the date of birth, `CreationTimestamp` seconds after
    /// the epoch, in US Eastern time (V312/V333). A non-self target is answered the same (V301: the
    /// target id is read; V301: the answer is always about the requester).
    #[test]
    fn query_birth_answers_with_the_date_of_birth() {
        for target in [0, 0x5000_0002, 0x5000_0103] {
            let mut h = super::fellowship::H::small();
            let sa = h.player(A, "Alpha", 3);
            in_world(&mut h, sa);
            // 2001-09-09 01:46:40 UTC, daylight saving: UTC-4
            h.w.objects
                .get_mut(A)
                .unwrap()
                .set_property(PropertyInt::CreationTimestamp, 1_000_000_000);
            start_capture();
            action(
                &mut h.w,
                sa,
                &CharacterQueryBirth {
                    target: ObjectId(target),
                },
            );
            assert_eq!(
                chats_to(&sent(), sa),
                ["You were born on 9/8/2001 9:46:40 PM."],
                "target {target:#X}"
            );
        }
    }

    /// V312/V333: `/birth` prints US Eastern time with the 2007 DST rule applied
    /// to every year. Every answer the retail captures quote (the birth-clock and
    /// query-age findings), then the changeover hours.
    #[test]
    fn query_birth_prints_us_eastern_with_the_2007_dst_rule_for_every_year() {
        use empyrean_common::dotnet::DotNetDateTime;
        use empyrean_world::network::game_action::actions::game_action_query_birth::birth_text;

        let utc = |y, mo, d, h, mi, s| {
            let t = DotNetDateTime::new_hms(y, mo, d, h, mi, s).ticks()
                - DotNetDateTime::UNIX_EPOCH.ticks();
            i32::try_from(t / 10_000_000).unwrap()
        };
        let cases = [
            // the gap births: DST under the 2007 rule, standard time under that year's historical rule
            (985_547_583, "3/25/2001 3:13:03 PM"),
            (1_018_044_225, "4/5/2002 6:03:45 PM"),
            (973_145_304, "11/2/2000 2:08:24 AM"),
            (941_575_636, "11/2/1999 4:47:16 PM"),
            // near the other boundaries
            (utc(2005, 3, 8, 0, 16, 20), "3/7/2005 7:16:20 PM"),
            (utc(2004, 4, 4, 14, 25, 33), "4/4/2004 10:25:33 AM"),
            (utc(2001, 11, 12, 9, 53, 55), "11/12/2001 4:53:55 AM"),
            // winter
            (1_042_942_768, "1/18/2003 9:19:28 PM"),
        ];
        for (stamp, when) in cases {
            assert_eq!(
                birth_text(stamp),
                format!("You were born on {when}."),
                "{stamp}"
            );
        }

        // the changeovers (not in the captures): 2:00 EST on 2001-03-11 becomes 3:00 EDT; 2:00 EDT on
        // 2001-11-04 becomes 1:00 EST; the same in 2017
        let changes = [
            (utc(2001, 3, 11, 6, 59, 59), "3/11/2001 1:59:59 AM"),
            (utc(2001, 3, 11, 7, 0, 0), "3/11/2001 3:00:00 AM"),
            (utc(2001, 11, 4, 5, 59, 59), "11/4/2001 1:59:59 AM"),
            (utc(2001, 11, 4, 6, 0, 0), "11/4/2001 1:00:00 AM"),
            (utc(2017, 3, 12, 7, 0, 0), "3/12/2017 3:00:00 AM"),
            (utc(2017, 11, 5, 5, 30, 0), "11/5/2017 1:30:00 AM"),
            (utc(2017, 11, 5, 6, 30, 0), "11/5/2017 1:30:00 AM"),
            (utc(2017, 1, 1, 4, 59, 59), "12/31/2016 11:59:59 PM"),
        ];
        for (stamp, when) in changes {
            assert_eq!(
                birth_text(stamp),
                format!("You were born on {when}."),
                "{stamp}"
            );
        }
    }

    /// `GameActionSetSingleCharacterOption`: the option is stored in the character's options.
    #[test]
    fn a_single_character_option_is_stored() {
        let mut h = super::fellowship::H::small();
        let sa = h.player(A, "Alpha", 3);
        in_world(&mut h, sa);
        assert!(!player_character::get_character_option(
            &h.w,
            A,
            CharacterOption::AutoRepeatAttacks
        ));
        action(
            &mut h.w,
            sa,
            &CharacterPlayerOptionChangedEvent {
                option: CharacterOption::AutoRepeatAttacks.0.cast_unsigned(),
                value: 1,
            },
        );
        assert!(player_character::get_character_option(
            &h.w,
            A,
            CharacterOption::AutoRepeatAttacks
        ));
        action(
            &mut h.w,
            sa,
            &CharacterPlayerOptionChangedEvent {
                option: CharacterOption::AutoRepeatAttacks.0.cast_unsigned(),
                value: 0,
            },
        );
        assert!(!player_character::get_character_option(
            &h.w,
            A,
            CharacterOption::AutoRepeatAttacks
        ));
    }

    /// `GameActionSetCharacterOptions` (0x01A1): ignored before `FirstEnterWorldDone`; afterwards the
    /// options words are stored (here the flags carry CharacterOptions2 only).
    #[test]
    fn character_options_are_stored_once_the_player_has_entered_the_world() {
        let mut h = super::fellowship::H::small();
        let sa = h.player(A, "Alpha", 3);
        in_world(&mut h, sa);
        // flags (CharacterOptions2), options1, numTab1Spells = 0, options2
        let mut body = Vec::new();
        for v in [0x40u32, 0x0000_0002, 0, 0x0000_0010] {
            body.extend_from_slice(&v.to_le_bytes());
        }
        let send = |w: &mut World| {
            let blob = pack_action_raw(0x10, Opcode(0x01A1), &body);
            handle_client_message(w, ClientMessage::new(blob).expect("opcode"), sa);
            run_inbound_message_queue(w);
        };
        let character = |w: &World| {
            let c = w
                .objects
                .get(A)
                .unwrap()
                .player
                .as_ref()
                .unwrap()
                .player
                .character
                .as_ref()
                .unwrap();
            (c.character_options_1, c.character_options_2)
        };
        let options1 = |w: &World| character(w).0;

        let before = options1(&h.w);
        send(&mut h.w);
        assert_eq!(
            options1(&h.w),
            before,
            "before FirstEnterWorldDone: ignored"
        );

        h.w.objects
            .get_mut(A)
            .unwrap()
            .set_first_enter_world_done(true);
        send(&mut h.w);
        assert_eq!(character(&h.w), (2, 0x10));
    }

    /// V350 (retail): the generic-quality list is read as the client writes it — a float entry is a
    /// key and an `f32`, a string entry a key and a packed string — so the options2 word and the window
    /// layout that follow it are applied. The body is the client's own player-module layout.
    #[test]
    fn character_options_with_float_and_string_qualities_keep_options2_and_the_layout() {
        use dereth_protocol::archive::PackedHash;
        use dereth_protocol::login::{
            player_module_flags as f, CharacterCharacterOptionsEvent, GenericQualitiesData,
            PlayerModule,
        };
        use dereth_protocol::property::PackObjPropertyCollection;

        let mut h = super::fellowship::H::small();
        let sa = h.player(A, "Alpha", 3);
        in_world(&mut h, sa);
        h.w.objects
            .get_mut(A)
            .unwrap()
            .set_first_enter_world_done(true);

        let layout = PackObjPropertyCollection::default();
        let module = PlayerModule {
            // What the client's own header construction always sets, plus the two sections under test.
            option_flags: f::SPELL_LISTS_8
                | f::SPELLBOOK_FILTERS
                | f::CHARACTER_OPTIONS_2
                | f::GENERIC_QUALITIES_DATA
                | f::GAMEPLAY_OPTIONS,
            options: 0x0000_0002,
            spell_bars: vec![
                vec![0x3E],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            ],
            spell_filters: 0x3FFF,
            options2: 0x0000_0010,
            generic_qualities: Some(GenericQualitiesData {
                flags: 0x1 | 0x4 | 0x8,
                option_ints: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(1, 7)],
                }),
                option_bools: None,
                // 0.3's low half is 0x999A: read as a string length it overruns the message.
                option_floats: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(2, 0.3)],
                }),
                option_strings: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(3, "hello".to_owned())],
                }),
            }),
            gameplay_options: Some(layout.clone()),
            ..PlayerModule::default()
        };
        action(&mut h.w, sa, &CharacterCharacterOptionsEvent { module });

        let c =
            h.w.objects
                .get(A)
                .unwrap()
                .player
                .as_ref()
                .unwrap()
                .player
                .character
                .as_ref()
                .unwrap();
        assert_eq!((c.character_options_1, c.character_options_2), (2, 0x10));
        let mut want = dereth_protocol::Writer::new();
        layout.write(&mut want).expect("encode");
        want.align4();
        assert_eq!(
            c.gameplay_options.as_deref(),
            Some(want.into_inner().as_slice()),
            "the window layout is saved"
        );
    }

    /// A player module whose generic-quality list carries a float entry as the client writes it
    /// (a key and an 8-byte double) still has its gameplay-options collection saved whole, a text
    /// property an interface keeps there included.
    #[test]
    #[ignore = "waits on the generic-quality float entry being read as 12 bytes (a key and a                 double), in the protocol codec and in this handler's skip"]
    fn a_double_float_quality_leaves_the_gameplay_options_and_their_text_property_whole() {
        use dereth_protocol::archive::PackedHash;
        use dereth_protocol::login::{
            player_module_flags as f, CharacterCharacterOptionsEvent, GenericQualitiesData,
            PlayerModule,
        };
        use dereth_protocol::property::{
            BaseProperty, BasePropertyValue, PackObjPropertyCollection, PropertyCollection,
        };

        let mut h = super::fellowship::H::small();
        let sa = h.player(A, "Alpha", 3);
        in_world(&mut h, sa);
        h.w.objects
            .get_mut(A)
            .unwrap()
            .set_first_enter_world_done(true);

        let text = "dxhb1;1;i5000000a,,s3e";
        let layout = PackObjPropertyCollection {
            properties: PropertyCollection {
                bucket_index: 0,
                entries: vec![(
                    0xE5,
                    BaseProperty {
                        name: 0xE5,
                        value: Some(BasePropertyValue::String(text.to_owned())),
                    },
                )],
            },
            ..PackObjPropertyCollection::default()
        };
        let module = PlayerModule {
            option_flags: f::SPELL_LISTS_8
                | f::SPELLBOOK_FILTERS
                | f::CHARACTER_OPTIONS_2
                | f::GENERIC_QUALITIES_DATA
                | f::GAMEPLAY_OPTIONS,
            spell_bars: vec![vec![]; 8],
            spell_filters: 0x3FFF,
            options2: 0x0000_0010,
            generic_qualities: Some(GenericQualitiesData {
                flags: 0x1,
                option_ints: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(1, 7)],
                }),
                option_bools: None,
                option_floats: None,
                option_strings: None,
            }),
            gameplay_options: Some(layout.clone()),
            ..PlayerModule::default()
        };
        // The list as the client writes it: the int table, then a float table whose entry is a
        // key and a double.
        let ints_only: Vec<u8> = [1u32, 1 | (8 << 16), 1, 7]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut with_float: Vec<u8> = [1u32 | 4, 1 | (8 << 16), 1, 7, 1 | (8 << 16), 2]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        with_float.extend_from_slice(&0.3f64.to_le_bytes());
        let blob = pack_action(0x10, &CharacterCharacterOptionsEvent { module }).expect("encode");
        let at = blob
            .windows(ints_only.len())
            .position(|w| w == ints_only.as_slice())
            .expect("the list is in the message");
        let mut blob2 = blob[..at].to_vec();
        blob2.extend_from_slice(&with_float);
        blob2.extend_from_slice(&blob[at + ints_only.len()..]);
        handle_client_message(&mut h.w, ClientMessage::new(blob2).expect("opcode"), sa);
        run_inbound_message_queue(&mut h.w);

        let c =
            h.w.objects
                .get(A)
                .unwrap()
                .player
                .as_ref()
                .unwrap()
                .player
                .character
                .as_ref()
                .unwrap();
        assert_eq!(c.character_options_2, 0x10);
        let mut want = dereth_protocol::Writer::new();
        layout.write(&mut want).expect("encode");
        want.align4();
        assert_eq!(
            c.gameplay_options.as_deref(),
            Some(want.into_inner().as_slice()),
            "the gameplay options are saved from their own start"
        );
    }
}
