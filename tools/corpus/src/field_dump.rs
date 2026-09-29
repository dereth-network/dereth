//! Write decoded fields from the shared corpus for the census tool.

use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_protocol::Message;
use std::fmt::Debug;
use std::io::Write;

enum Decoded {
    Ok(String),
    Trailing(String, usize),
    NoDecoder,
    AceZeroBlob,
    Failed(String),
}

fn d<M: Message + Debug>(body: &[u8]) -> Decoded {
    match dereth_protocol::read_body_padded::<M>(body) {
        Ok(m) => Decoded::Ok(format!("{m:#?}")),
        Err(e) => {
            let mut r = dereth_protocol::Reader::body(body);
            match M::read(&mut r) {
                Ok(m) => Decoded::Trailing(format!("{m:#?}"), r.remaining()),
                Err(_) => Decoded::Failed(format!("{e:?}")),
            }
        }
    }
}

fn decode(dir: Direction, op: u32, body: &[u8]) -> Decoded {
    match (Some(dir), op) {
        (_, 0x0004) => d::<dereth_protocol::comms::CommunicationPopUpString>(body),
        (_, 0x0005) => d::<dereth_protocol::login::CharacterPlayerOptionChangedEvent>(body),
        (_, 0x0008) => d::<dereth_protocol::combat::CombatTargetedMeleeAttack>(body),
        (_, 0x000A) => d::<dereth_protocol::combat::CombatTargetedMissileAttack>(body),
        (_, 0x0013) => d::<dereth_protocol::login::LoginPlayerDescription>(body),
        (_, 0x0015) => d::<dereth_protocol::comms::CommunicationTalk>(body),
        (_, 0x0017) => d::<dereth_protocol::social::SocialRemoveFriend>(body),
        (_, 0x0018) => d::<dereth_protocol::social::SocialAddFriend>(body),
        (_, 0x0019) => d::<dereth_protocol::items::InventoryPutItemInContainer>(body),
        (_, 0x001A) => d::<dereth_protocol::items::InventoryGetAndWieldItem>(body),
        (_, 0x001B) => d::<dereth_protocol::items::InventoryDropItem>(body),
        (_, 0x001D) => d::<dereth_protocol::social::AllegianceSwearAllegiance>(body),
        (_, 0x001E) => d::<dereth_protocol::social::AllegianceBreakAllegiance>(body),
        (_, 0x001F) => d::<dereth_protocol::social::AllegianceUpdateRequest>(body),
        (_, 0x0020) => d::<dereth_protocol::social::AllegianceUpdate>(body),
        (_, 0x0021) => d::<dereth_protocol::social::SocialFriendsUpdate>(body),
        (_, 0x0022) => d::<dereth_protocol::objects::ItemServerSaysContainId>(body),
        (_, 0x0023) => d::<dereth_protocol::objects::ItemWearItem>(body),
        (_, 0x0024) => d::<dereth_protocol::objects::ItemServerSaysRemove>(body),
        (_, 0x0029) => d::<dereth_protocol::social::CharacterTitlesMessage>(body),
        (_, 0x0035) => d::<dereth_protocol::items::InventoryUseWithTargetEvent>(body),
        (_, 0x0036) => d::<dereth_protocol::items::InventoryUseEvent>(body),
        (_, 0x0044) => d::<dereth_protocol::admin::TrainAttribute2nd>(body),
        (_, 0x0045) => d::<dereth_protocol::admin::TrainAttribute>(body),
        (_, 0x0046) => d::<dereth_protocol::admin::TrainSkill>(body),
        (_, 0x0047) => d::<dereth_protocol::admin::TrainSkillAdvancementClass>(body),
        (_, 0x004A) => d::<dereth_protocol::combat::MagicCastTargetedSpell>(body),
        (_, 0x0052) => d::<dereth_protocol::objects::ItemStopViewingObjectContents>(body),
        (_, 0x0053) => d::<dereth_protocol::combat::CombatChangeCombatMode>(body),
        (_, 0x0054) => d::<dereth_protocol::items::InventoryStackableMerge>(body),
        (_, 0x0055) => d::<dereth_protocol::items::InventoryStackableSplitToContainer>(body),
        (_, 0x0058) => d::<dereth_protocol::comms::CommunicationModifyCharacterSquelch>(body),
        (_, 0x0059) => d::<dereth_protocol::comms::CommunicationModifyAccountSquelch>(body),
        (_, 0x005D) => d::<dereth_protocol::comms::CommunicationTalkDirectByName>(body),
        (_, 0x005F) => d::<dereth_protocol::trade::VendorBuy>(body),
        (_, 0x0060) => d::<dereth_protocol::trade::VendorSell>(body),
        (_, 0x0062) => d::<dereth_protocol::trade::VendorInfo>(body),
        (_, 0x0063) => d::<dereth_protocol::combat::CharacterTeleToLifestone>(body),
        (_, 0x00A0) => d::<dereth_protocol::objects::CharacterServerSaysAttemptFailed>(body),
        (_, 0x00A1) => d::<dereth_protocol::login::CharacterLoginCompleteNotification>(body),
        (_, 0x00A2) => d::<dereth_protocol::social::FellowshipCreate>(body),
        (Some(Direction::ServerToClient), 0x00A3) => {
            d::<dereth_protocol::social::FellowshipQuitNotice>(body)
        }
        (Some(Direction::ClientToServer), 0x00A3) => {
            d::<dereth_protocol::social::FellowshipQuitRequest>(body)
        }
        (_, 0x00A4) => d::<dereth_protocol::social::FellowshipDismiss>(body),
        (_, 0x00A5) => d::<dereth_protocol::social::FellowshipRecruit>(body),
        (_, 0x00A6) => d::<dereth_protocol::social::FellowshipUpdateRequest>(body),
        (_, 0x00B4) => d::<dereth_protocol::trade::WritingBookOpen>(body),
        (_, 0x00C8) => d::<dereth_protocol::objects::ItemAppraise>(body),
        (_, 0x00C9) => d::<dereth_protocol::objects::ItemSetAppraiseInfo>(body),
        (_, 0x00CD) => d::<dereth_protocol::items::InventoryGiveObjectRequest>(body),
        (Some(Direction::ServerToClient), 0x0147) => {
            d::<dereth_protocol::comms::CommunicationChannelBroadcastRecv>(body)
        }
        (Some(Direction::ClientToServer), 0x0147) => {
            d::<dereth_protocol::comms::CommunicationChannelBroadcast>(body)
        }
        (_, 0x0195) => d::<dereth_protocol::objects::InventoryNoLongerViewingContents>(body),
        (_, 0x0196) => d::<dereth_protocol::objects::ItemOnViewContents>(body),
        (_, 0x0197) => d::<dereth_protocol::items::ItemUpdateStackSize>(body),
        (_, 0x019A) => d::<dereth_protocol::objects::ItemServerSaysMoveItem>(body),
        (_, 0x019C) => d::<dereth_protocol::login::CharacterAddShortCut>(body),
        (_, 0x019D) => d::<dereth_protocol::login::CharacterRemoveShortCut>(body),
        (_, 0x019E) => d::<dereth_protocol::combat::CombatHandlePlayerDeathEvent>(body),
        (_, 0x01A1) => d::<dereth_protocol::login::CharacterCharacterOptionsEvent>(body),
        (_, 0x01A7) => d::<dereth_protocol::combat::CombatHandleAttackDoneEvent>(body),
        (_, 0x01AC) => d::<dereth_protocol::combat::VictimNotificationSelf>(body),
        (_, 0x01AD) => d::<dereth_protocol::combat::VictimNotificationOther>(body),
        (_, 0x01B1) => d::<dereth_protocol::combat::AttackerNotification>(body),
        (_, 0x01B2) => d::<dereth_protocol::combat::DefenderNotification>(body),
        (_, 0x01B3) => d::<dereth_protocol::combat::EvasionAttackerNotification>(body),
        (_, 0x01B4) => d::<dereth_protocol::combat::EvasionDefenderNotification>(body),
        (_, 0x01B7) => d::<dereth_protocol::combat::CombatCancelAttack>(body),
        (_, 0x01B8) => d::<dereth_protocol::combat::CombatHandleCommenceAttackEvent>(body),
        (_, 0x01BF) => d::<dereth_protocol::combat::CombatQueryHealth>(body),
        (_, 0x01C0) => d::<dereth_protocol::combat::CombatQueryHealthResponse>(body),
        (_, 0x01C7) => d::<dereth_protocol::objects::ItemUseDone>(body),
        (_, 0x01C9) => d::<dereth_protocol::social::FellowshipFellowUpdateDone>(body),
        (_, 0x01DF) => d::<dereth_protocol::comms::CommunicationEmote>(body),
        (_, 0x01E0) => d::<dereth_protocol::comms::CommunicationHearEmote>(body),
        (_, 0x01E1) => d::<dereth_protocol::comms::CommunicationSoulEmote>(body),
        (_, 0x01E2) => d::<dereth_protocol::comms::CommunicationHearSoulEmote>(body),
        (_, 0x01E3) => d::<dereth_protocol::combat::CharacterAddSpellFavorite>(body),
        (_, 0x01E4) => d::<dereth_protocol::combat::CharacterRemoveSpellFavorite>(body),
        (_, 0x01F4) => d::<dereth_protocol::comms::CommunicationSetSquelchDb>(body),
        (_, 0x01F6) => d::<dereth_protocol::trade::TradeOpenTradeNegotiations>(body),
        (_, 0x01F7) => d::<dereth_protocol::trade::TradeCloseTradeNegotiations>(body),
        (_, 0x01F8) => d::<dereth_protocol::trade::TradeAddToTrade>(body),
        (_, 0x01FA) => d::<dereth_protocol::trade::TradeAcceptTradeRequest>(body),
        (_, 0x01FD) => d::<dereth_protocol::trade::TradeRegisterTrade>(body),
        (_, 0x01FF) => d::<dereth_protocol::trade::TradeCloseTrade>(body),
        (_, 0x0200) => d::<dereth_protocol::trade::TradeAddToTradeRecv>(body),
        (_, 0x0202) => d::<dereth_protocol::trade::TradeAcceptTradeRecv>(body),
        (_, 0x0204) => d::<dereth_protocol::trade::TradeResetTradeRequest>(body),
        (_, 0x0205) => d::<dereth_protocol::trade::TradeResetTradeRecv>(body),
        (_, 0x0207) => d::<dereth_protocol::trade::TradeTradeFailure>(body),
        (_, 0x021C) => d::<dereth_protocol::trade::HouseBuyHouse>(body),
        (_, 0x021D) => d::<dereth_protocol::trade::HouseProfileMessage>(body),
        (_, 0x021E) => d::<dereth_protocol::trade::HouseQueryHouse>(body),
        (_, 0x021F) => d::<dereth_protocol::trade::HouseAbandonHouse>(body),
        (_, 0x0221) => d::<dereth_protocol::trade::HouseRentHouse>(body),
        (_, 0x0225) => d::<dereth_protocol::trade::HouseDataMessage>(body),
        (_, 0x0226) => d::<dereth_protocol::trade::HouseHouseStatus>(body),
        (_, 0x0245) => d::<dereth_protocol::trade::HouseAddPermanentGuest>(body),
        (_, 0x0246) => d::<dereth_protocol::trade::HouseRemovePermanentGuest>(body),
        (_, 0x0247) => d::<dereth_protocol::trade::HouseSetOpenHouseStatus>(body),
        (_, 0x0248) => d::<dereth_protocol::trade::HouseUpdateRestrictions>(body),
        (_, 0x0249) => d::<dereth_protocol::trade::HouseChangeStoragePermission>(body),
        (_, 0x024A) => d::<dereth_protocol::trade::HouseBootSpecificHouseGuest>(body),
        (_, 0x024C) => d::<dereth_protocol::trade::HouseRemoveAllStoragePermission>(body),
        (_, 0x024D) => d::<dereth_protocol::trade::HouseRequestFullGuestList>(body),
        (_, 0x0257) => d::<dereth_protocol::trade::HouseUpdateHar>(body),
        (_, 0x025E) => d::<dereth_protocol::trade::HouseRemoveAllPermanentGuests>(body),
        (_, 0x0262) => d::<dereth_protocol::trade::HouseTeleToHouse>(body),
        (_, 0x0263) => d::<dereth_protocol::items::ItemQueryItemMana>(body),
        (_, 0x0264) => d::<dereth_protocol::items::ItemQueryItemManaResponse>(body),
        (_, 0x0266) => d::<dereth_protocol::trade::HouseSetHooksVisibility>(body),
        (_, 0x0267) => d::<dereth_protocol::trade::HouseModifyAllegianceGuestPermission>(body),
        (_, 0x0268) => d::<dereth_protocol::trade::HouseModifyAllegianceStoragePermission>(body),
        (_, 0x0274) => d::<dereth_protocol::comms::CharacterConfirmationRequest>(body),
        (_, 0x0275) => d::<dereth_protocol::comms::CharacterConfirmationResponse>(body),
        (_, 0x0278) => d::<dereth_protocol::trade::HouseTeleToMansion>(body),
        (_, 0x027D) => d::<dereth_protocol::items::InventoryCreateTinkeringTool>(body),
        (_, 0x028A) => d::<dereth_protocol::comms::CommunicationWeenieError>(body),
        (_, 0x028B) => d::<dereth_protocol::comms::CommunicationWeenieErrorWithString>(body),
        (_, 0x0290) => d::<dereth_protocol::social::FellowshipAssignNewLeader>(body),
        (_, 0x0291) => d::<dereth_protocol::social::FellowshipChangeFellowOpenness>(body),
        (_, 0x0295) => d::<dereth_protocol::comms::ChatRoomMembership>(body),
        (_, 0x02BB) => d::<dereth_protocol::comms::CommunicationHearSpeech>(body),
        (_, 0x02BD) => d::<dereth_protocol::comms::CommunicationHearDirectSpeech>(body),
        (_, 0x02BE) => d::<dereth_protocol::social::FellowshipFullUpdate>(body),
        (_, 0x02BF) => d::<dereth_protocol::social::FellowshipDisband>(body),
        (_, 0x02C0) => d::<dereth_protocol::social::FellowshipUpdateFellow>(body),
        (_, 0x02C1) => d::<dereth_protocol::qualities::MagicUpdateSpell>(body),
        (_, 0x02C2) => d::<dereth_protocol::qualities::MagicUpdateEnchantment>(body),
        (_, 0x02C3) => d::<dereth_protocol::qualities::MagicRemoveEnchantment>(body),
        (_, 0x02C6) => d::<dereth_protocol::qualities::MagicPurgeEnchantments>(body),
        (_, 0x02C7) => d::<dereth_protocol::qualities::MagicDispelEnchantment>(body),
        (_, 0x02CD) => d::<dereth_protocol::qualities::QualitiesPrivateUpdateInt>(body),
        (_, 0x02CE) => d::<dereth_protocol::qualities::QualitiesUpdateInt>(body),
        (_, 0x02CF) => d::<dereth_protocol::qualities::QualitiesPrivateUpdateInt64>(body),
        (_, 0x02D2) => d::<dereth_protocol::qualities::QualitiesUpdateBool>(body),
        (_, 0x02D6) => d::<dereth_protocol::qualities::QualitiesUpdateString>(body),
        (_, 0x02D9) => d::<dereth_protocol::qualities::QualitiesPrivateUpdateInstanceId>(body),
        (_, 0x02DA) => d::<dereth_protocol::qualities::QualitiesUpdateInstanceId>(body),
        (_, 0x02DB) => d::<dereth_protocol::qualities::QualitiesPrivateUpdatePosition>(body),
        (_, 0x02DD) => d::<dereth_protocol::qualities::QualitiesPrivateUpdateSkill>(body),
        (_, 0x02E3) => d::<dereth_protocol::qualities::QualitiesPrivateUpdateAttribute>(body),
        (_, 0x02E7) => d::<dereth_protocol::qualities::QualitiesPrivateUpdateAttribute2nd>(body),
        (_, 0x02E9) => {
            d::<dereth_protocol::qualities::QualitiesPrivateUpdateAttribute2ndLevel>(body)
        }
        (_, 0x02EB) => d::<dereth_protocol::comms::CommunicationTransientString>(body),
        (_, 0xF61B) => d::<dereth_protocol::movement::MovementJump>(body),
        (_, 0xF61C) => d::<dereth_protocol::movement::MovementMoveToState>(body),
        (_, 0xF625) => d::<dereth_protocol::objects::ItemObjDescEvent>(body),
        (_, 0xF643) => d::<dereth_protocol::login::CharGenVerificationResponse>(body),
        (Some(Direction::ClientToServer), 0xF653) => {
            d::<dereth_protocol::login::LoginExecuteLogOffRequest>(body)
        }
        (Some(Direction::ServerToClient), 0xF653) => {
            d::<dereth_protocol::login::LoginExecuteLogOff>(body)
        }
        (Some(Direction::ClientToServer), 0xF655) => {
            d::<dereth_protocol::login::CharacterDeleteRequest>(body)
        }
        (Some(Direction::ServerToClient), 0xF655) => {
            d::<dereth_protocol::login::CharacterDeleteAck>(body)
        }
        (_, 0xF656) => d::<dereth_protocol::login::CharacterSendCharGenResult>(body),
        (_, 0xF657) => d::<dereth_protocol::login::LoginSendEnterWorld>(body),
        (_, 0xF658) => d::<dereth_protocol::login::LoginCharacterSet>(body),
        (_, 0xF6EA) => d::<dereth_protocol::objects::ObjectSendForceObjdesc>(body),
        (_, 0xF745) => d::<dereth_protocol::objects::ItemCreateObject>(body),
        (_, 0xF746) => d::<dereth_protocol::objects::LoginCreatePlayer>(body),
        (_, 0xF747) => d::<dereth_protocol::objects::ItemDeleteObject>(body),
        (_, 0xF748) => d::<dereth_protocol::movement::MovementPositionEvent>(body),
        (_, 0xF749) => d::<dereth_protocol::objects::ItemParentEvent>(body),
        (_, 0xF74A) => d::<dereth_protocol::objects::InventoryPickupEvent>(body),
        (_, 0xF74B) => d::<dereth_protocol::objects::ItemSetState>(body),
        (_, 0xF74C) => d::<dereth_protocol::movement::MovementSetObjectMovement>(body),
        (_, 0xF74E) => d::<dereth_protocol::movement::MovementVectorUpdate>(body),
        (_, 0xF750) => d::<dereth_protocol::objects::EffectsSoundEvent>(body),
        (_, 0xF751) => d::<dereth_protocol::objects::EffectsPlayerTeleport>(body),
        (_, 0xF753) => d::<dereth_protocol::movement::MovementAutonomousPosition>(body),
        (_, 0xF755) => d::<dereth_protocol::objects::EffectsPlayScriptType>(body),
        (_, 0xF7C8) => d::<dereth_protocol::login::LoginSendEnterWorldRequest>(body),
        (_, 0xF7D9) => d::<dereth_protocol::admin::AdminSendAdminRestoreCharacter>(body),
        (_, 0xF7DB) => d::<dereth_protocol::objects::ItemUpdateObject>(body),
        (_, 0xF7DC) => d::<dereth_protocol::login::LoginAccountBooted>(body),
        (_, 0xF7DE) => d::<dereth_protocol::comms::CommunicationTurbineChat>(body),
        (_, 0xF7DF) => d::<dereth_protocol::login::LoginEnterGameServerReady>(body),
        (_, 0xF7E0) => d::<dereth_protocol::comms::CommunicationTextboxString>(body),
        (_, 0xF7E1) => d::<dereth_protocol::login::LoginWorldInfo>(body),
        (_, 0xF7E5) => d::<dereth_protocol::admin::DddInterrogation>(body),
        (_, 0xF7E6) => d::<dereth_protocol::admin::DddInterrogationResponse>(body),
        (_, 0xF7EA) => d::<dereth_protocol::admin::DddEndDdd>(body),
        (_, 0x0000) if body.iter().all(|&b| b == 0) => Decoded::AceZeroBlob, // 1
        _ => Decoded::NoDecoder,
    }
}

fn derived(op: u32, body: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if op == 0xF61C {
        if let Ok(m) = dereth_protocol::read_body_padded::<
            dereth_protocol::movement::MovementMoveToState,
        >(body)
        {
            if let Ok(f) = m.0.raw_motion_state.flags() {
                out.push(("raw_motion_state.flags".to_string(), format!("0x{f:X}")));
            }
            out.push((
                "raw_motion_state.actions.len".to_string(),
                format!("{}", m.0.raw_motion_state.actions.len()),
            ));
        }
    }
    out
}

fn extra_records(op: u32, body: &[u8]) -> Vec<(&'static str, Decoded)> {
    let mut out = Vec::new();
    if op == 0xF74C {
        if let Ok(m) = dereth_protocol::read_body_padded::<
            dereth_protocol::movement::MovementSetObjectMovement,
        >(body)
        {
            out.push((
                "MovementBuffer",
                match m.decoded_movement() {
                    Ok(b) => Decoded::Ok(format!("{b:#?}")),
                    Err(e) => Decoded::Failed(format!("{e:?}")),
                },
            ));
        }
    }
    out
}

fn split(b: &CorpusBlob) -> (&'static str, u32, &[u8]) {
    let p = &b.payload;
    if b.opcode == dereth_protocol::OrderedActionHeader::MAGIC && p.len() >= 12 {
        (
            "action",
            u32::from_le_bytes([p[8], p[9], p[10], p[11]]),
            &p[12..],
        )
    } else if b.opcode == dereth_protocol::OrderedEventHeader::MAGIC && p.len() >= 16 {
        (
            "event",
            u32::from_le_bytes([p[12], p[13], p[14], p[15]]),
            &p[16..],
        )
    } else {
        ("bare", b.opcode, if p.len() >= 4 { &p[4..] } else { &[] })
    }
}

fn name_of(op: u32) -> &'static str {
    dereth_protocol::Opcode(op).name().unwrap_or("?")
}

pub(crate) fn run() {
    // The dump is this run's output: `corpus_census.txt` in cargo's target directory, the folder
    // above the profile folder that holds this executable.
    let out_path = std::env::var("DERETH_TEST_CENSUS_OUT").unwrap_or_else(|_| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.parent()?.join("corpus_census.txt")))
            .unwrap_or_else(|| std::env::temp_dir().join("corpus_census.txt"))
            .to_string_lossy()
            .into_owned()
    });
    if let Some(dir) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(dir).expect("the output directory must be creatable");
    }
    let f = std::fs::File::create(&out_path).expect("the dump must be writable");
    let mut w = std::io::BufWriter::new(f);

    let (mut s2c, mut c2s) = (0usize, 0usize);
    let (mut ok, mut nodec, mut failed, mut trailing) = (0usize, 0usize, 0usize, 0usize);
    let mut zero_blob = 0usize;
    let mut ran = 0usize;
    for corpus in Corpus::shared_all() {
        let scen = &corpus.name;
        ran += 1;
        for b in &corpus.blobs {
            match b.dir {
                Direction::ServerToClient => s2c += 1,
                Direction::ClientToServer => c2s += 1,
            }
            let (wrap, op, body) = split(b);
            let dir = match b.dir {
                Direction::ServerToClient => "s2c",
                Direction::ClientToServer => "c2s",
            };
            let dec = decode(b.dir, op, body);
            let mut left_over = None;
            let (status, text) = match &dec {
                Decoded::Ok(s) => {
                    ok += 1;
                    ("ok", s.as_str())
                }
                Decoded::Trailing(s, n) => {
                    trailing += 1;
                    left_over = Some(*n);
                    ("ok-trailing", s.as_str())
                }
                Decoded::NoDecoder => {
                    nodec += 1;
                    ("no-decoder", "")
                }
                Decoded::AceZeroBlob => {
                    zero_blob += 1;
                    ("ace-zero-blob", "")
                }
                Decoded::Failed(s) => {
                    failed += 1;
                    ("failed", s.as_str())
                }
            };
            let mut extra = derived(op, body)
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(" | ");
            if let Some(n) = left_over {
                if !extra.is_empty() {
                    extra.push_str(" | ");
                }
                extra.push_str(&format!("trailing_bytes={n}"));
            }
            writeln!(
                w,
                "### {scen} {} {dir} {wrap} 0x{op:04X} {} {status}{}{extra}",
                b.idx,
                name_of(op),
                if extra.is_empty() { "" } else { " " }
            )
            .expect("write");
            for line in text.lines() {
                assert!(
                    !line.starts_with("###"),
                    "a Debug body produced a line that looks like a record header; the dump \
                     format is ambiguous and every count taken from it is suspect"
                );
                writeln!(w, "{line}").expect("write");
            }
            for (tag, sub) in extra_records(op, body) {
                let (status, text) = match &sub {
                    Decoded::Ok(s) => ("ok", s.as_str()),
                    Decoded::Trailing(s, _) => ("ok-trailing", s.as_str()),
                    Decoded::NoDecoder => ("no-decoder", ""),
                    Decoded::AceZeroBlob => ("ace-zero-blob", ""),
                    Decoded::Failed(s) => ("failed", s.as_str()),
                };
                writeln!(
                    w,
                    "### {scen} {} {dir} sub 0x{op:04X} {tag} {status}",
                    b.idx
                )
                .expect("write");
                for line in text.lines() {
                    writeln!(w, "{line}").expect("write");
                }
            }
        }
    }
    w.flush().expect("flush");

    eprintln!(
        "corpus census: {ran} scenarios | {s2c} server + {c2s} client blobs | {ok} decoded, \
         {trailing} decoded with bytes left over, {nodec} with no decoder, {zero_blob} ACE zero \
         blob, {failed}          failed to decode | -> {out_path}"
    );
    assert!(
        ran > 0 && s2c > 0 && c2s > 0,
        "the corpus must contain messages in both directions"
    );
    assert!(ok + trailing > 0, "the decoder must produce fields");
}
