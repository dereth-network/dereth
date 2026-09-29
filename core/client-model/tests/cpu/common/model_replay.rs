//! Shared fixtures for model replay.

#![allow(dead_code, unused_imports)]

pub(crate) use crate::common::corpus;
pub(crate) use crate::common::corpus_size::recorded_world_sessions;

pub(crate) use corpus::{Blob, Dir};
pub(crate) use dereth_client_model::inventory::requests::InventoryRequest;
pub(crate) use dereth_client_model::inventory::SplitState;
pub(crate) use dereth_client_model::qualities::{StatKey, StatType, StatValue};
pub(crate) use dereth_client_model::{
    Notice, NullRequests, RecordingRequests, RecordingSink, Request, RequestSink, World,
};
pub(crate) use dereth_primitives::{LocalTime, ObjectId, ServerTime};
pub(crate) use dereth_protocol::actions::{pack_action, unpack_action};
pub(crate) use dereth_protocol::events::split_ui_blob;
pub(crate) use dereth_protocol::{read_body, read_body_padded, Message, MessageError, Reader};

pub(crate) const OP_ITEM_CREATE_OBJECT: u32 = 0xF745;
pub(crate) const OP_LOGIN_CREATE_PLAYER: u32 = 0xF746;
pub(crate) const OP_ENTER_GAME_SERVER_READY: u32 = 0xF7DF;
pub(crate) const OP_ITEM_DELETE_OBJECT: u32 = 0xF747;
pub(crate) const OP_ITEM_UPDATE_OBJECT: u32 = 0xF7DB;
pub(crate) const OP_GAME_EVENT: u32 = 0xF7B0;
pub(crate) const OP_GAME_ACTION: u32 = 0xF7B1;

pub(crate) const EV_PLAYER_DESCRIPTION: u32 = 0x0013;
pub(crate) const EV_SERVER_SAYS_CONTAIN_ID: u32 = 0x0022;
pub(crate) const EV_WEAR_ITEM: u32 = 0x0023;
pub(crate) const EV_ATTEMPT_FAILED: u32 = 0x00A0;
pub(crate) const EV_SET_APPRAISE_INFO: u32 = 0x00C9;
pub(crate) const EV_ON_VIEW_CONTENTS: u32 = 0x0196;
pub(crate) const EV_STOP_VIEWING: u32 = 0x0052;
pub(crate) const EV_UPDATE_ENCHANTMENT: u32 = 0x02C2;
pub(crate) const EV_ALLEGIANCE_UPDATE: u32 = 0x0020;
pub(crate) const EV_FRIENDS_UPDATE: u32 = 0x0021;
pub(crate) const EV_CHARACTER_TITLE_TABLE: u32 = 0x0029;

pub(crate) const AC_PUT_ITEM_IN_CONTAINER: u32 = 0x0019;
pub(crate) const AC_GET_AND_WIELD_ITEM: u32 = 0x001A;
pub(crate) const AC_APPRAISE: u32 = 0x00C8;
pub(crate) const AC_GIVE_OBJECT_REQUEST: u32 = 0x00CD;
pub(crate) const AC_USE_EVENT: u32 = 0x0036;
pub(crate) const AC_USE_WITH_TARGET: u32 = 0x0035;
pub(crate) const AC_CHANGE_COMBAT_MODE: u32 = 0x0053;
pub(crate) const AC_QUERY_HEALTH: u32 = 0x01BF;
pub(crate) const AC_CAST_TARGETED_SPELL: u32 = 0x004A;
pub(crate) const AC_CANCEL_ATTACK: u32 = 0x01B7;

#[derive(Debug, Default)]
pub(crate) struct Replay {
    pub(crate) world_objects: usize,
    pub(crate) player: Option<ObjectId>,
    pub(crate) creates: usize,
    pub(crate) merges: usize,
    pub(crate) stale_instances: usize,
    pub(crate) stat_updates: usize,
    pub(crate) stat_updates_without_player: usize,
    pub(crate) stat_updates_stale: usize,
    pub(crate) stat_updates_before_create: usize,
    pub(crate) enchantments: usize,
    pub(crate) appraisals: usize,
    pub(crate) player_descriptions: usize,
    pub(crate) player_modules_decoded: usize,
    pub(crate) create_players: usize,
    pub(crate) deletes: usize,
    pub(crate) allegiance_versions: std::collections::BTreeSet<u32>,
    pub(crate) allegiance_members: usize,
    pub(crate) friend_updates: usize,
    pub(crate) titles: usize,
    pub(crate) title_tables: usize,
    pub(crate) events_seen: std::collections::BTreeMap<u32, usize>,
    pub(crate) notices: Vec<Notice>,
    pub(crate) early: Vec<(u32, ObjectId, StatKey)>,
    pub(crate) created_ever: std::collections::BTreeSet<ObjectId>,
    pub(crate) latched: Vec<ObjectId>,
}

pub(crate) fn replay(blobs: &[Blob]) -> Replay {
    let mut w = World::new();
    let mut out = RecordingSink::default();
    let mut req = NullRequests;
    let mut r = Replay::default();

    for b in blobs.iter().filter(|b| b.dir == Dir::S2c) {
        let now = ServerTime(b.t);
        match b.opcode() {
            OP_ENTER_GAME_SERVER_READY => {
                w = World::new();
            }
            OP_LOGIN_CREATE_PLAYER => {
                let m = read_body_padded::<dereth_protocol::objects::LoginCreatePlayer>(b.body())
                    .expect("every 0xF746 in the corpus must decode");
                w.set_player(m.player_id);
                r.create_players += 1;
                if let Some(p) = w.player {
                    r.latched.push(p);
                }
            }
            OP_ITEM_CREATE_OBJECT => {
                let m = read_body::<dereth_protocol::objects::ItemCreateObject>(b.body())
                    .expect("every 0xF745 in the corpus must decode with the cursor exhausted");
                r.created_ever.insert(m.0.id);
                match w.create_or_merge(&m.0, now, &mut out) {
                    Ok(_) => r.creates += 1,
                    Err(dereth_client_model::GameError::DuplicateCreate(_)) => r.merges += 1,
                    Err(dereth_client_model::GameError::StaleInstance { .. }) => {
                        r.stale_instances += 1
                    }
                    Err(e) => panic!("unexpected create failure: {e}"),
                }
            }
            OP_ITEM_UPDATE_OBJECT => {
                let m = read_body::<dereth_protocol::objects::ItemUpdateObject>(b.body())
                    .expect("every 0xF7DB in the corpus must decode with the cursor exhausted");
                let _ = w.recreate(&m.0, now, &mut out);
            }
            OP_ITEM_DELETE_OBJECT => {
                let m = read_body_padded::<dereth_protocol::objects::ItemDeleteObject>(b.body())
                    .expect("every 0xF747 in the corpus must decode, allowing the align padding");
                w.server_says_remove(m.id, now, &mut out);
                r.deletes += 1;
            }
            OP_GAME_EVENT => {
                let Ok(ui) = split_ui_blob(&b.payload) else {
                    continue;
                };
                *r.events_seen.entry(ui.sub_type.0).or_insert(0) += 1;
                apply_event(&mut w, ui.sub_type.0, ui.body, now, &mut out, &mut r);
            }
            op => apply_quality(&mut w, op, b.body(), now, &mut out, &mut r),
        }
        w.use_time(now, &mut out, &mut req);
    }

    r.world_objects = w.tables.weenies.len();
    r.player = w.player;
    r.enchantments = w
        .player_qualities()
        .map_or(0, |q| q.enchantments.enchantments_in_effect().len());
    r.titles = w.player_system.social.titles.len();
    r.notices = out.0;
    r
}

pub(crate) fn body_of<M: Message>(mut r: Reader<'_>) -> Result<M, MessageError> {
    let m = M::read(&mut r)?;
    r.expect_exhausted()?;
    Ok(m)
}

pub(crate) fn apply_event(
    w: &mut World,
    sub: u32,
    body: Reader<'_>,
    now: ServerTime,
    out: &mut RecordingSink,
    r: &mut Replay,
) {
    match sub {
        EV_PLAYER_DESCRIPTION => {
            let mut rr = body;
            let q = dereth_protocol::types::qualities::AcQualities::read(&mut rr)
                .expect("the ACQualities prefix of Login_PlayerDescription must decode");
            r.player_descriptions += 1;
            let pm = dereth_protocol::login::PlayerModule::read(&mut rr)
                .expect("every PlayerModule in the corpus must decode; #205 is resolved");
            r.player_modules_decoded += 1;
            w.player_system.apply_player_module(&pm);
            if let Some(p) = w.player {
                if let Some(weenie) = w.weenie_mut(p) {
                    let qual = weenie
                        .qualities
                        .get_or_insert_with(dereth_client_model::Qualities::new);
                    qual.apply_ac_qualities(&q, LocalTime(now.0));
                }
            }
        }
        EV_SERVER_SAYS_CONTAIN_ID => {
            let m = body_of::<dereth_protocol::objects::ItemServerSaysContainId>(body)
                .expect("Item_ServerSaysContainID must decode with the cursor exhausted");
            w.server_says_move_item(m.item, m.container, m.slot, ObjectId(0), 0, true, out);
        }
        EV_WEAR_ITEM => {
            let m = body_of::<dereth_protocol::objects::ItemWearItem>(body)
                .expect("Item_WearItem must decode with the cursor exhausted");
            let player = w.player.unwrap_or_default();
            w.server_says_move_item(m.item, ObjectId(0), 0, player, m.slot, true, out);
        }
        EV_ON_VIEW_CONTENTS => {
            let m = body_of::<dereth_protocol::objects::ItemOnViewContents>(body)
                .expect("Item_OnViewContents must decode with the cursor exhausted");
            w.view_object_contents(m.container, &m.contents, out);
        }
        EV_STOP_VIEWING => {
            let m = body_of::<dereth_protocol::objects::ItemStopViewingObjectContents>(body)
                .expect("Item_StopViewingObjectContents must decode with the cursor exhausted");
            w.stop_viewing_object_contents(m.object, out);
        }
        EV_SET_APPRAISE_INFO => {
            let m = body_of::<dereth_protocol::objects::ItemSetAppraiseInfo>(body)
                .expect("Item_SetAppraiseInfo must decode with the cursor exhausted");
            r.appraisals += 1;
            w.set_appraise_info(m.object, m.profile, out);
        }
        EV_ATTEMPT_FAILED => {
            let m = body_of::<dereth_protocol::objects::CharacterServerSaysAttemptFailed>(body)
                .expect("Character_ServerSaysAttemptFailed must decode with the cursor exhausted");
            w.server_says_attempt_failed(m.object, m.reason, out);
        }
        EV_ALLEGIANCE_UPDATE => {
            let m = body_of::<dereth_protocol::social::AllegianceUpdate>(body)
                .expect("Allegiance_AllegianceUpdate must decode with the cursor exhausted");
            r.allegiance_versions.insert(m.profile.hierarchy.version);
            r.allegiance_members = w.handle_allegiance_update(&m.profile) as usize;
        }
        EV_FRIENDS_UPDATE => {
            let m = body_of::<dereth_protocol::social::SocialFriendsUpdate>(body)
                .expect("Social_FriendsUpdate must decode with the cursor exhausted");
            r.friend_updates += 1;
            let _ = m;
        }
        EV_CHARACTER_TITLE_TABLE => {
            let m = body_of::<dereth_protocol::social::CharacterTitlesMessage>(body)
                .expect("Social_CharacterTitleTable must decode with the cursor exhausted");
            assert_eq!(m.version, 1, "the initial title-table version");
            r.title_tables += 1;
            w.player_system.social.display_title = m.display_title;
            w.player_system.social.titles = m.titles;
        }
        EV_UPDATE_ENCHANTMENT => {
            if let Ok(m) = body_of::<dereth_protocol::qualities::MagicUpdateEnchantment>(body) {
                if let Some(q) = w.player_qualities_mut() {
                    q.update_enchantment(&m.0, LocalTime(now.0));
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn apply_quality(
    w: &mut World,
    op: u32,
    body: &[u8],
    _now: ServerTime,
    out: &mut RecordingSink,
    r: &mut Replay,
) {
    use dereth_protocol::qualities as q;
    macro_rules! private {
        ($ty:ty, $stat:expr, $conv:expr) => {{
            let m = read_body_padded::<$ty>(body).expect("recorded quality update decodes");
            let Some(player) = w.player else {
                r.stat_updates_without_player += 1;
                return;
            };
            let key = StatKey::new($stat, m.0.property_id);
            #[allow(clippy::redundant_closure_call)]
            let v = ($conv)(m.0.value);
            let known = w.weenie(player).is_some();
            if w.apply_stat_update(player, key, v, m.0.sequence, out) {
                r.stat_updates += 1;
            } else if known {
                r.stat_updates_stale += 1;
            } else {
                r.stat_updates_before_create += 1;
            }
        }};
    }
    macro_rules! public {
        ($ty:ty, $stat:expr, $conv:expr) => {{
            let m = read_body_padded::<$ty>(body).expect("recorded quality update decodes");
            let key = StatKey::new($stat, m.0.property_id);
            #[allow(clippy::redundant_closure_call)]
            let v = ($conv)(m.0.value);
            let known = w.weenie(m.0.object).is_some();
            if w.apply_stat_update(m.0.object, key, v, m.0.sequence, out) {
                r.stat_updates += 1;
            } else if known {
                r.stat_updates_stale += 1;
            } else {
                r.early.push((op, m.0.object, key));
                r.stat_updates_before_create += 1;
            }
        }};
    }
    match op {
        0x02CD => private!(q::QualitiesPrivateUpdateInt, StatType::Int, StatValue::Int),
        0x02CE => public!(q::QualitiesUpdateInt, StatType::Int, StatValue::Int),
        0x02CF => private!(
            q::QualitiesPrivateUpdateInt64,
            StatType::Int64,
            StatValue::Int64
        ),
        0x02D0 => public!(q::QualitiesUpdateInt64, StatType::Int64, StatValue::Int64),
        0x02D1 => private!(q::QualitiesPrivateUpdateBool, StatType::Bool, |v: i32| {
            StatValue::Bool(v != 0)
        }),
        0x02D2 => public!(q::QualitiesUpdateBool, StatType::Bool, |v: i32| {
            StatValue::Bool(v != 0)
        }),
        0x02D3 => private!(
            q::QualitiesPrivateUpdateFloat,
            StatType::Float,
            StatValue::Float
        ),
        0x02D4 => public!(q::QualitiesUpdateFloat, StatType::Float, StatValue::Float),
        0x02D7 => private!(q::QualitiesPrivateUpdateDataId, StatType::Did, |v: u32| {
            StatValue::Did(dereth_primitives::DataId(v))
        }),
        0x02D8 => public!(q::QualitiesUpdateDataId, StatType::Did, |v: u32| {
            StatValue::Did(dereth_primitives::DataId(v))
        }),
        0x02D9 => private!(
            q::QualitiesPrivateUpdateInstanceId,
            StatType::Iid,
            StatValue::Iid
        ),
        0x02DA => public!(q::QualitiesUpdateInstanceId, StatType::Iid, StatValue::Iid),
        _ => {}
    }
}

pub(crate) fn reproduce(sub: u32, body: Reader<'_>) -> Option<Request> {
    let mut w = World::new();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let t = ServerTime(0.0);

    match sub {
        AC_PUT_ITEM_IN_CONTAINER => {
            let m = body_of::<dereth_protocol::items::InventoryPutItemInContainer>(body).ok()?;
            w.attempt_put_in_container(&mut req, &mut out, m.item, m.container, m.slot, t, true)
                .ok()?;
        }
        AC_GET_AND_WIELD_ITEM => {
            let m = body_of::<dereth_protocol::items::InventoryGetAndWieldItem>(body).ok()?;
            w.attempt_wield(
                &mut req,
                &mut out,
                m.item,
                m.slot,
                SplitState::default(),
                t,
                true,
            )
            .ok()?;
        }
        AC_GIVE_OBJECT_REQUEST => {
            let m = body_of::<dereth_protocol::items::InventoryGiveObjectRequest>(body).ok()?;
            w.attempt_give(&mut req, &mut out, m.item, m.target, m.amount, t, true)
                .ok()?;
        }
        AC_APPRAISE => {
            let m = body_of::<dereth_protocol::objects::ItemAppraise>(body).ok()?;
            w.attempt_appraise(&mut req, m.target);
        }
        AC_USE_EVENT => {
            let m = body_of::<dereth_protocol::items::InventoryUseEvent>(body).ok()?;
            w.attempt_use(&mut req, m.object);
        }
        AC_USE_WITH_TARGET => {
            let m = body_of::<dereth_protocol::items::InventoryUseWithTargetEvent>(body).ok()?;
            w.attempt_use_with_target(&mut req, m.object, m.target);
        }
        AC_QUERY_HEALTH => {
            let m = body_of::<dereth_protocol::combat::CombatQueryHealth>(body).ok()?;
            w.query_health(&mut req, m.target);
        }
        AC_CHANGE_COMBAT_MODE => {
            let m = body_of::<dereth_protocol::combat::CombatChangeCombatMode>(body).ok()?;
            let mode = dereth_client_model::combat::CombatMode::from_raw(m.combat_mode);
            w.inventory_mask = 0xFFFF_FFFF;
            w.combat.combat_mode = if m.combat_mode == 1 {
                dereth_client_model::combat::CombatMode::from_raw(2)
            } else {
                dereth_client_model::combat::CombatMode::from_raw(1)
            };
            w.set_combat_mode(&mut req, &mut out, mode, true, true, false)
                .ok()?;
        }
        AC_CANCEL_ATTACK => {
            let _ = body_of::<dereth_protocol::combat::CombatCancelAttack>(body).ok()?;
            w.combat.attack_request_in_progress = true;
            w.abort_automatic_attack(&mut req);
        }
        AC_CAST_TARGETED_SPELL => {
            let m = body_of::<dereth_protocol::combat::MagicCastTargetedSpell>(body).ok()?;
            req.send(Request::CastTargetedSpell(m));
        }
        _ => return None,
    }
    req.0.into_iter().next()
}

pub(crate) fn encode(stamp: u32, r: &Request) -> Vec<u8> {
    fn b<M: Message>(stamp: u32, m: &M) -> Vec<u8> {
        pack_action(stamp, m).expect("every request this crate produces must encode")
    }
    match r {
        Request::GetAndWieldItem(m) => b(stamp, m),
        Request::StackableSplitToWield(m) => b(stamp, m),
        Request::PutItemInContainer(m) => b(stamp, m),
        Request::DropItem(m) => b(stamp, m),
        Request::StackableMerge(m) => b(stamp, m),
        Request::StackableSplitToContainer(m) => b(stamp, m),
        Request::StackableSplitTo3d(m) => b(stamp, m),
        Request::GiveObjectRequest(m) => b(stamp, m),
        Request::Appraise(m) => b(stamp, m),
        Request::UseEvent(m) => b(stamp, m),
        Request::UseWithTargetEvent(m) => b(stamp, m),
        Request::CreateTinkeringTool(m) => b(stamp, m),
        Request::ForceObjdesc(m) => b(stamp, m),
        Request::ChangeCombatMode(m) => b(stamp, m),
        Request::TargetedMeleeAttack(m) => b(stamp, m),
        Request::TargetedMissileAttack(m) => b(stamp, m),
        Request::CancelAttack(m) => b(stamp, m),
        Request::QueryHealth(m) => b(stamp, m),
        Request::CastUntargetedSpell(m) => b(stamp, m),
        Request::CastTargetedSpell(m) => b(stamp, m),
        _ => Vec::new(),
    }
}

pub(crate) fn name_of(n: &Notice) -> &'static str {
    match n {
        Notice::SelectionChanged { .. } => "SelectionChanged",
        Notice::ObjectCreated(_) => "ObjectCreated",
        Notice::ObjectDeleted(_) => "ObjectDeleted",
        Notice::StatUpdated(..) => "StatUpdated",
        Notice::StatRemoved(..) => "StatRemoved",
        Notice::ItemAttributesChanged { .. } => "ItemAttributesChanged",
        Notice::ItemMoved { .. } => "ItemMoved",
        Notice::AttemptFailed { .. } => "AttemptFailed",
        Notice::InventoryChanged(_) => "InventoryChanged",
        Notice::ShowPendingInPlayer(_) => "ShowPendingInPlayer",
        Notice::EndPendingInPlayer => "EndPendingInPlayer",
        Notice::AppraisalReady(_) => "AppraisalReady",
        Notice::OpenBook(_) => "OpenBook",
        Notice::CloseBook(_) => "CloseBook",
        Notice::EnchantmentsChanged => "EnchantmentsChanged",
        Notice::VitaeChanged => "VitaeChanged",
        Notice::PlayerObjDescChanged => "PlayerObjDescChanged",
        Notice::DisplayString { .. } => "DisplayString",
        Notice::OpenContainedContainer(_) => "OpenContainedContainer",
        Notice::SetGroundObject(_) => "SetGroundObject",
        Notice::OpenSalvagePanel(_) => "OpenSalvagePanel",
        Notice::BeginGame(_) => "BeginGame",
        Notice::UsageConfirmation { .. } => "UsageConfirmation",
        Notice::TargetedUsageConfirmation { .. } => "TargetedUsageConfirmation",
        Notice::OpenVendor { .. } => "OpenVendor",
        Notice::CloseVendor => "CloseVendor",
        Notice::CloseSlumlord(_) => "CloseSlumlord",
        Notice::AddItemToSell(_) => "AddItemToSell",
        Notice::TradeRegistered { .. } => "TradeRegistered",
        Notice::OpenSecureTrade { .. } => "OpenSecureTrade",
        Notice::CloseSecureTrade { .. } => "CloseSecureTrade",
        Notice::TradeItemAdded { .. } => "TradeItemAdded",
        Notice::TradeItemRemoved { .. } => "TradeItemRemoved",
        Notice::TradeAccepted { .. } => "TradeAccepted",
        Notice::TradeDeclined { .. } => "TradeDeclined",
        Notice::TradeReset { .. } => "TradeReset",
        Notice::TradeFailure { .. } => "TradeFailure",
        Notice::TradeAcceptanceCleared => "TradeAcceptanceCleared",
        Notice::TradeAnItemForDummies(_) => "TradeAnItemForDummies",
    }
}
