//! Vectors: fixtures/vectors/messages/
//! Ported builders write ACE bytes and the protocol crate reads/rewrites them byte-for-byte;
//! KNOWN differences table exact; two objects' icons in retail short form.
//! Fixture: ACE vectors and explicit expected values.

// V236.

use std::cell::Cell;

use dereth_protocol::{self as dp, Message, Reader, Writer};
use empyrean_common::vectors;
use serde_json::Value;

use super::messages::build_vector_case;

type RoundTrip = fn(&[u8]) -> Result<Vec<u8>, String>;

/// ACE's short-form range for a known-type DataID: `WritePackedDword` takes 2 bytes up to 32767.
const ACE_KNOWN_TYPE_SHORT_LIMIT: u32 = 0x8000;

thread_local! {
    /// The short-form range this thread's round trips write with: retail's, as the server writes,
    /// except inside [`at_short_limit`].
    static SHORT_LIMIT: Cell<u32> = const { Cell::new(Writer::RETAIL_KNOWN_TYPE_SHORT_LIMIT) };
}

/// `f` with this thread's round trips writing at short-form range `limit`.
fn at_short_limit<T>(limit: u32, f: impl FnOnce() -> T) -> T {
    let before = SHORT_LIMIT.replace(limit);
    let out = f();
    SHORT_LIMIT.set(before);
    out
}

/// Some sub-record writers cannot fail and return `()`; the rest return a `Result`.
trait Written {
    fn written(self) -> Result<(), dp::MessageError>;
}

impl Written for () {
    fn written(self) -> Result<(), dp::MessageError> {
        Ok(())
    }
}

impl Written for Result<(), dp::MessageError> {
    fn written(self) -> Result<(), dp::MessageError> {
        self
    }
}

/// Reads `body` as `$t` (every byte consumed) and writes it back.
macro_rules! sub_record {
    ($t:ty) => {
        |body: &[u8]| -> Result<Vec<u8>, String> {
            let mut r = Reader::body(body);
            let m = <$t>::read(&mut r).map_err(|e| format!("read: {e}"))?;
            // At most three zero bytes may be left: the sender's alignment padding, which the
            // retail reader never looks at ([`Outcome::Padded`]).
            if r.remaining() >= 4 || r.rest().iter().any(|b| *b != 0) {
                return Err(format!("{} bytes left", r.remaining()));
            }
            let mut w = Writer::body().with_known_type_short_limit(SHORT_LIMIT.get());
            Written::written(m.write(&mut w)).map_err(|e| format!("write: {e}"))?;
            Ok(w.into_inner())
        }
    };
}

/// [`sub_record!`] for a message body.
fn rt<M: Message>(body: &[u8]) -> Result<Vec<u8>, String> {
    sub_record!(M)(body)
}

/// The dereth-protocol codec each ACE class's body is read with. `None`: the class has no retail
/// counterpart (see [`KNOWN`]).
fn codec(class: &str) -> Option<RoundTrip> {
    Some(match class {
        "GameEventAcceptTrade" => rt::<dp::trade::TradeAcceptTradeRecv>,
        "GameEventAddToTrade" => rt::<dp::trade::TradeAddToTradeRecv>,
        "GameEventAllegianceAllegianceUpdateDone" => rt::<dp::social::AllegianceUpdateDone>,
        "GameEventAllegianceLoginNotification" => rt::<dp::social::AllegianceLoginNotification>,
        "GameEventAttackDone" => rt::<dp::combat::CombatHandleAttackDoneEvent>,
        "GameEventAttackerNotification" => rt::<dp::combat::AttackerNotification>,
        "GameEventBookAddPageResponse" => rt::<dp::trade::WritingBookAddPageResponse>,
        "GameEventBookDeletePageResponse" => rt::<dp::trade::WritingBookDeletePageResponse>,
        "GameEventBookModifyPageResponse" => rt::<dp::trade::WritingBookModifyPageResponse>,
        "GameEventChannelBroadcast" => rt::<dp::comms::CommunicationChannelBroadcastRecv>,
        "GameEventClearTradeAcceptance" => rt::<dp::trade::TradeClearTradeAcceptance>,
        "GameEventCloseGroundContainer" => rt::<dp::objects::ItemStopViewingObjectContents>,
        "GameEventCloseTrade" => rt::<dp::trade::TradeCloseTrade>,
        "GameEventCombatCommenceAttack" => rt::<dp::combat::CombatHandleCommenceAttackEvent>,
        "GameEventCommunicationTransientString" => rt::<dp::comms::CommunicationTransientString>,
        "GameEventConfirmationDone" => rt::<dp::comms::CharacterConfirmationDone>,
        "GameEventConfirmationRequest" => rt::<dp::comms::CharacterConfirmationRequest>,
        "GameEventDeclineTrade" => rt::<dp::trade::TradeDeclineTradeRecv>,
        "GameEventDefenderNotification" => rt::<dp::combat::DefenderNotification>,
        "GameEventEvasionAttackerNotification" => rt::<dp::combat::EvasionAttackerNotification>,
        "GameEventEvasionDefenderNotification" => rt::<dp::combat::EvasionDefenderNotification>,
        "GameEventFellowshipDisband" => rt::<dp::social::FellowshipDisband>,
        "GameEventFellowshipDismiss" => rt::<dp::social::FellowshipDismiss>,
        "GameEventFellowshipFellowUpdateDone" => rt::<dp::social::FellowshipFellowUpdateDone>,
        "GameEventFellowshipQuit" => rt::<dp::social::FellowshipQuitNotice>,
        "GameEventGameOver" => rt::<dp::trade::GameGameOver>,
        "GameEventHouseStatus" => rt::<dp::trade::HouseHouseStatus>,
        "GameEventHouseTransaction" => rt::<dp::trade::HouseHouseTransaction>,
        "GameEventHouseUpdateRentTime" => rt::<dp::trade::HouseUpdateRentTime>,
        "GameEventInventoryServerSaveFailed" => rt::<dp::objects::CharacterServerSaysAttemptFailed>,
        "GameEventItemServerSaysMoveItem" => rt::<dp::objects::ItemServerSaysMoveItem>,
        "GameEventJoinGameResponse" => rt::<dp::trade::GameJoinGameResponse>,
        "GameEventKillerNotification" => rt::<dp::combat::VictimNotificationOther>,
        "GameEventMagicDispelEnchantment" => rt::<dp::qualities::MagicDispelEnchantment>,
        "GameEventMagicPurgeBadEnchantments" => rt::<dp::qualities::MagicPurgeBadEnchantments>,
        "GameEventMagicPurgeEnchantments" => rt::<dp::qualities::MagicPurgeEnchantments>,
        "GameEventMagicRemoveEnchantment" => rt::<dp::qualities::MagicRemoveEnchantment>,
        "GameEventMagicRemoveSpell" => rt::<dp::qualities::MagicRemoveSpell>,
        "GameEventMagicUpdateSpell" => rt::<dp::qualities::MagicUpdateSpell>,
        "GameEventMoveResponse" => rt::<dp::trade::GameMoveResponse>,
        "GameEventOpponentStalemate" => rt::<dp::trade::GameOpponentStalemateState>,
        "GameEventPingResponse" => rt::<dp::admin::CharacterReturnPing>,
        "GameEventPopupString" => rt::<dp::comms::CommunicationPopUpString>,
        "GameEventPortalStormBrewing" => rt::<dp::trade::MiscPortalStormBrewing>,
        "GameEventPortalStormImminent" => rt::<dp::trade::MiscPortalStormImminent>,
        "GameEventPortalStormSubsided" => rt::<dp::trade::MiscPortalStormSubsided>,
        "GameEventPortalStorm" => rt::<dp::trade::MiscPortalStorm>,
        "GameEventQueryAgeResponse" => rt::<dp::admin::CharacterQueryAgeResponse>,
        "GameEventQueryItemManaResponse" => rt::<dp::items::ItemQueryItemManaResponse>,
        "GameEventRegisterTrade" => rt::<dp::trade::TradeRegisterTrade>,
        "GameEventResetTrade" => rt::<dp::trade::TradeResetTradeRecv>,
        "GameEventSetTurbineChatChannels" => rt::<dp::comms::ChatRoomMembership>,
        "GameEventStartGame" => rt::<dp::trade::GameStartGame>,
        "GameEventTell" => rt::<dp::comms::CommunicationHearDirectSpeech>,
        "GameEventTradeFailure" => rt::<dp::trade::TradeTradeFailure>,
        "GameEventUpdateHealth" => rt::<dp::combat::CombatQueryHealthResponse>,
        "GameEventUpdateTitle" => rt::<dp::social::SocialAddOrSetCharacterTitle>,
        "GameEventUseDone" => rt::<dp::objects::ItemUseDone>,
        "GameEventVictimNotification" => rt::<dp::combat::VictimNotificationSelf>,
        "GameEventWeenieErrorWithString" => rt::<dp::comms::CommunicationWeenieErrorWithString>,
        "GameEventWeenieError" => rt::<dp::comms::CommunicationWeenieError>,
        "GameEventWieldItem" => rt::<dp::objects::ItemWearItem>,
        "GameMessageAdminEnvirons" => rt::<dp::admin::AdminEnvirons>,
        "GameMessageBootAccount" => rt::<dp::login::LoginAccountBooted>,
        "GameMessageCharacterCreateResponse" => rt::<dp::login::CharGenVerificationResponse>,
        "GameMessageCharacterDelete" => rt::<dp::login::CharacterDeleteAck>,
        "GameMessageCharacterEnterWorldServerReady" => rt::<dp::login::LoginEnterGameServerReady>,
        "GameMessageCharacterError" => rt::<dp::login::CharacterError>,
        "GameMessageCharacterLogOff" => rt::<dp::login::LoginExecuteLogOff>,
        "GameMessageCharacterRestore" => rt::<dp::login::CharGenVerificationResponse>,
        "GameMessageDDDBeginDDD" => rt::<dp::admin::DddBeginDdd>,
        "GameMessageDDDEndDDD" => rt::<dp::admin::DddEndDdd>,
        "GameMessageDDDErrorMessage" => rt::<dp::admin::DddError>,
        "GameMessageDeleteObject" => rt::<dp::objects::ItemDeleteObject>,
        "GameMessageEmoteText" => rt::<dp::comms::CommunicationHearEmote>,
        "GameMessageHearRangedSpeech" => rt::<dp::comms::CommunicationHearRangedSpeech>,
        "GameMessageHearSpeech" => rt::<dp::comms::CommunicationHearSpeech>,
        "GameMessageInventoryRemoveObject" => rt::<dp::objects::ItemServerSaysRemove>,
        "GameMessageParentEvent" => rt::<dp::objects::ItemParentEvent>,
        "GameMessagePickupEvent" => rt::<dp::objects::InventoryPickupEvent>,
        "GameMessagePlayerCreate" => rt::<dp::objects::LoginCreatePlayer>,
        "GameMessagePlayerKilled" => rt::<dp::combat::CombatHandlePlayerDeathEvent>,
        "GameMessagePrivateUpdateAttribute2ndLevel" => {
            rt::<dp::qualities::QualitiesPrivateUpdateAttribute2ndLevel>
        }
        "GameMessagePrivateUpdateDataID" => rt::<dp::qualities::QualitiesPrivateUpdateDataId>,
        "GameMessagePrivateUpdateInstanceID" => {
            rt::<dp::qualities::QualitiesPrivateUpdateInstanceId>
        }
        "GameMessagePrivateUpdatePosition" => rt::<dp::qualities::QualitiesPrivateUpdatePosition>,
        "GameMessagePrivateUpdatePropertyBool" => rt::<dp::qualities::QualitiesPrivateUpdateBool>,
        "GameMessagePrivateUpdatePropertyFloat" => rt::<dp::qualities::QualitiesPrivateUpdateFloat>,
        "GameMessagePrivateUpdatePropertyInt64" => rt::<dp::qualities::QualitiesPrivateUpdateInt64>,
        "GameMessagePrivateUpdatePropertyInt" => rt::<dp::qualities::QualitiesPrivateUpdateInt>,
        "GameMessagePrivateUpdatePropertyString" => {
            rt::<dp::qualities::QualitiesPrivateUpdateString>
        }
        "GameMessagePublicUpdateInstanceID" => rt::<dp::qualities::QualitiesUpdateInstanceId>,
        "GameMessagePublicUpdatePosition" => rt::<dp::qualities::QualitiesUpdatePosition>,
        "GameMessagePublicUpdatePropertyBool" => rt::<dp::qualities::QualitiesUpdateBool>,
        "GameMessagePublicUpdatePropertyDataID" => rt::<dp::qualities::QualitiesUpdateDataId>,
        "GameMessagePublicUpdatePropertyFloat" => rt::<dp::qualities::QualitiesUpdateFloat>,
        "GameMessagePublicUpdatePropertyInt64" => rt::<dp::qualities::QualitiesUpdateInt64>,
        "GameMessagePublicUpdatePropertyInt" => rt::<dp::qualities::QualitiesUpdateInt>,
        "GameMessagePublicUpdatePropertyString" => rt::<dp::qualities::QualitiesUpdateString>,
        "GameMessagePublicUpdateVital" => rt::<dp::qualities::QualitiesUpdateAttribute2nd>,
        "GameMessageScript" => rt::<dp::objects::EffectsPlayScriptType>,
        "GameMessageServerName" => rt::<dp::login::LoginWorldInfo>,
        "GameMessageSetState" => rt::<dp::objects::ItemSetState>,
        "GameMessageSoulEmote" => rt::<dp::comms::CommunicationHearSoulEmote>,
        "GameMessageSound" => rt::<dp::objects::EffectsSoundEvent>,
        "GameMessageSystemChat" => rt::<dp::comms::CommunicationTextboxString>,
        "GameMessageTurbineChat" => rt::<dp::comms::CommunicationTurbineChat>,
        "GameMessageVectorUpdate" => rt::<dp::movement::MovementVectorUpdate>,
        _ => return None,
    })
}

/// What dereth-protocol makes of one case's ACE bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// It reads the body and writes it back unchanged.
    Same,
    /// It reads a prefix and leaves this many bytes over.
    Rejects(usize),
    /// It reads the body and writes it back without ACE's trailing zero padding of this many
    /// bytes.
    Padded(usize),
    /// It reads the body and writes it back this many bytes longer.
    Longer(usize),
    /// It has no codec for the class (a message the retail client does not know).
    NoCodec,
}

/// The known ACE-vs-retail differences: (vector key, outcome, why). The key is the ACE class for
/// the message vectors and `world_objects.<message>.<object guid>` for the object vectors.
///
/// None remain: the fellowship lock table, the last, is in dereth-protocol since the retail captures
/// showed the retail server sending it (V256/V279).
const KNOWN: &[(&str, Outcome, &str)] = &[];

/// Messages that use retail's layout: ACE's recorded bytes are the record of what ACE
/// sent, and this gives the bytes the builder now writes instead. `None`: the builder writes ACE's
/// bytes.
pub(crate) fn retail_ruled(class: &str, ace: &[u8]) -> Option<Vec<u8>> {
    match class {
        // V234: the object guid before the property (opcode, seq, then the two dwords).
        "GameMessagePublicUpdatePropertyString" => {
            let mut v = ace.to_vec();
            let (prop, guid) = (v[5..9].to_vec(), v[9..13].to_vec());
            v[5..9].copy_from_slice(&guid);
            v[9..13].copy_from_slice(&prop);
            Some(v)
        }
        // V235/V293 (revised, V293): AttackConditions is eight bytes, as ACE and retail's server
        // write it, so ACE's bytes stand for 0x01B1/0x01B2.
        // V239: each "bytes to follow" (after the opcode, and after the seven header words) counts
        // exactly the bytes that follow it, 8 fewer than ACE's. The vectors' sender names are under
        // 128 characters and their messages under 256, where ACE's length prefixes are retail's.
        "GameMessageTurbineChat" if ace.len() > 4 => {
            let mut v = ace.to_vec();
            for at in [4, 36] {
                let n = u32::from_le_bytes(v[at..at + 4].try_into().ok()?) - 8;
                v[at..at + 4].copy_from_slice(&n.to_le_bytes());
            }
            Some(v)
        }
        // V236: any known-type DataID in 0x4000..=0x7FFF (none of the message vectors has one).
        _ => retail_packed(codec(class)?, ace, ace.len() - body(ace).len()),
    }
}

/// [`retail_ruled`] for one `serialization_structures` kind. `None`: ACE's bytes stand.
///
/// V279: a fellowship lock table's entries in retail's
/// hash-bucket order, the name's string hash (the spell table's, computed here by dereth-assets,
/// not the codec under test) mod the table size, ACE's order kept within a bucket.
pub(crate) fn retail_ruled_structure(kind: &str, ace: &[u8]) -> Option<Vec<u8>> {
    if kind != "fellowship_locks" {
        return None;
    }
    let count = usize::from(u16::from_le_bytes([ace[0], ace[1]]));
    let table_size = u32::from(u16::from_le_bytes([ace[2], ace[3]]));
    let mut at = 4;
    let mut entries: Vec<(u32, &[u8])> = Vec::with_capacity(count);
    for _ in 0..count {
        let start = at;
        let len = usize::from(u16::from_le_bytes([ace[at], ace[at + 1]]));
        let name = &ace[at + 2..at + 2 + len];
        at += (2 + len).next_multiple_of(4) + 20;
        entries.push((
            dereth_assets::tables::spell_hash(name) % table_size,
            &ace[start..at],
        ));
    }
    entries.sort_by_key(|(bucket, _)| *bucket);
    let mut v = ace[..4].to_vec();
    for (_, entry) in entries {
        v.extend_from_slice(entry);
    }
    v.extend_from_slice(&ace[at..]);
    Some(v)
}

/// [`retail_ruled`] for one of `serialization_world_objects`' outputs (`key`: a message, or
/// `game_data`, the object's guid then its weenie description): V236's packing.
pub(crate) fn retail_ruled_object(key: &str, ace: &[u8]) -> Option<Vec<u8>> {
    retail_packed(world_object_codec(key), ace, ace.len() - body(ace).len())
}

// V236, V237, V334.
#[derive(Debug, Default)]
pub(crate) struct ObjectIds {
    pub(crate) icon: u32,
    pub(crate) overlay: u32,
    pub(crate) underlay: u32,
    pub(crate) palette: u32,
    pub(crate) subpalettes: Vec<u32>,
    pub(crate) textures: Vec<(u32, u32)>,
    pub(crate) parts: Vec<u32>,
    pub(crate) pscript: Option<u16>,
}

impl ObjectIds {
    fn objdesc(&self, d: &mut dp::types::ObjDesc) {
        if !d.subpalettes.is_empty() {
            d.palette_id = self.palette;
        }
        assert_eq!(d.subpalettes.len(), self.subpalettes.len(), "sub-palettes");
        assert_eq!(
            d.texture_changes.len(),
            self.textures.len(),
            "texture changes"
        );
        assert_eq!(d.anim_part_changes.len(), self.parts.len(), "part changes");
        for (s, v) in d.subpalettes.iter_mut().zip(&self.subpalettes) {
            s.sub_id = *v;
        }
        for (t, (old, new)) in d.texture_changes.iter_mut().zip(&self.textures) {
            (t.old_tex_id, t.new_tex_id) = (*old, *new);
        }
        for (a, v) in d.anim_part_changes.iter_mut().zip(&self.parts) {
            a.part_id = *v;
        }
    }

    fn wdesc(&self, d: &mut dp::types::PublicWeenieDesc) {
        d.icon_id = self.icon;
        if let Some(o) = d.icon_overlay_id.as_mut() {
            *o = self.overlay;
        }
        if let Some(u) = d.icon_underlay_id.as_mut() {
            *u = self.underlay;
        }
        d.pscript = self.pscript;
        let bit = dp::types::weeniedesc::header::PSCRIPT;
        d.header = if self.pscript.is_some() {
            d.header | bit
        } else {
            d.header & !bit
        };
    }
}

/// V236/V237: one of `serialization_world_objects`' outputs (`key`) as the server writes it with the
/// object's known-type DataIDs `ids`: ACE's bytes read back, those ids put in, and written with
/// dereth-protocol's default (retail) writer. `None` when ACE's bytes read back to the same ids, so the
/// case is still compared to ACE's bytes ([`retail_ruled_object`]).
///
/// ACE's bytes cannot be read back to the ids it was given: its pack subtracts the type's base
/// only from a value sharing a bit with it, so an offset of 0x1002 may be 0x04001002 or 0x1002.
/// Hence the ids come from the object.
pub(crate) fn retail_ruled_object_ids(key: &str, ace: &[u8], ids: &ObjectIds) -> Option<Vec<u8>> {
    fn rewrite<M: Message + PartialEq + Clone>(
        b: &[u8],
        put: impl FnOnce(&mut M),
    ) -> Option<Vec<u8>> {
        let read = dp::read_body_padded::<M>(b).expect("ACE's bytes decode");
        let mut m = read.clone();
        put(&mut m);
        (m != read).then(|| dp::write_body(&m).expect("encodes"))
    }
    let header = ace.len() - body(ace).len();
    let b = &ace[header..];
    let written = match key {
        "create" | "create_admin" => rewrite::<dp::objects::ItemCreateObject>(b, |m| {
            ids.objdesc(&mut m.0.objdesc);
            ids.wdesc(&mut m.0.wdesc);
        }),
        "update" => rewrite::<dp::objects::ItemUpdateObject>(b, |m| {
            ids.objdesc(&mut m.0.objdesc);
            ids.wdesc(&mut m.0.wdesc);
        }),
        "obj_desc" => rewrite::<dp::objects::ItemObjDescEvent>(b, |m| ids.objdesc(&mut m.objdesc)),
        "game_data" => {
            let mut r = Reader::body(b);
            let read = dp::types::PublicWeenieDesc::read(&mut r).expect("ACE's bytes decode");
            let mut desc = read.clone();
            ids.wdesc(&mut desc);
            (desc != read).then(|| {
                let mut w = Writer::body();
                desc.write(&mut w).expect("encodes");
                w.into_inner()
            })
        }
        other => panic!("no known-type DataIDs in {other}"),
    }?;
    let mut v = ace[..header].to_vec();
    v.extend(written);
    Some(v)
}

/// V236: ACE's bytes `ace` with each known-type DataID packed at the retail client's short-form
/// range (the 2-byte form only below 0x4000) instead of ACE's (up to 0x7FFF): the `header` bytes
/// are kept and the rest is read and rewritten by `f`, its dereth-protocol codec. `None` when the two ranges write the same bytes, so every
/// case the ruling does not touch is still compared to ACE's bytes as recorded.
///
/// # Panics
/// When the ranges differ on a body dereth-protocol does not rewrite byte for byte at ACE's range (short
/// of ACE's trailing alignment), so the change could not be the packing alone.
fn retail_packed(f: RoundTrip, ace: &[u8], header: usize) -> Option<Vec<u8>> {
    let b = &ace[header..];
    let at_ace = at_short_limit(ACE_KNOWN_TYPE_SHORT_LIMIT, || f(b)).ok()?;
    let at_retail = at_short_limit(Writer::RETAIL_KNOWN_TYPE_SHORT_LIMIT, || f(b)).ok()?;
    if at_ace == at_retail {
        return None;
    }
    let tail = &b[at_ace.len().min(b.len())..];
    assert!(
        b.starts_with(&at_ace) && tail.iter().all(|x| *x == 0),
        "the short-form range changes a body dereth-protocol does not rewrite: {}",
        hex(ace)
    );
    let mut v = ace[..header].to_vec();
    v.extend(at_retail);
    if !tail.is_empty() {
        // ACE's `Align()` after the last field.
        v.resize(v.len().next_multiple_of(4), 0);
    }
    Some(v)
}

fn known(key: &str) -> Outcome {
    KNOWN
        .iter()
        .find(|(k, _, _)| *k == key)
        .map_or(Outcome::Same, |(_, o, _)| *o)
}

use crate::support::hex::hex;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

/// What dereth-protocol makes of `body`.
fn outcome(f: Option<RoundTrip>, body: &[u8]) -> Result<Outcome, String> {
    let Some(f) = f else {
        return Ok(Outcome::NoCodec);
    };
    match f(body) {
        Ok(out) if out == body => Ok(Outcome::Same),
        Ok(out)
            if out.len() < body.len()
                && body.starts_with(&out)
                && body[out.len()..].iter().all(|b| *b == 0) =>
        {
            Ok(Outcome::Padded(body.len() - out.len()))
        }
        Ok(out) if out.len() > body.len() => Ok(Outcome::Longer(out.len() - body.len())),
        Ok(out) => Err(format!(
            "re-encodes differently: ACE {} dereth-protocol {}",
            hex(body),
            hex(&out)
        )),
        Err(e) => match e.strip_suffix(" bytes left").and_then(|n| n.parse().ok()) {
            Some(n) => Ok(Outcome::Rejects(n)),
            None => Err(e),
        },
    }
}

/// Checks one case against [`KNOWN`]; returns the failure, if any.
fn check(key: &str, f: Option<RoundTrip>, body: &[u8]) -> Option<String> {
    let got = match outcome(f, body) {
        Ok(o) => o,
        Err(e) => return Some(format!("{key}: {e}")),
    };
    let want = known(key);
    (got != want).then(|| format!("{key}: dereth-protocol gives {got:?}, the table says {want:?}"))
}

/// The body of one message's bytes: after the opcode, or after the 16-byte header of a game event.
fn body(data: &[u8]) -> &[u8] {
    if data[..4] == 0xF7B0u32.to_le_bytes() {
        &data[16..]
    } else {
        &data[4..]
    }
}

#[test]
fn the_ported_builders_write_aces_bytes_and_dereth_protocol_reads_and_rewrites_them() {
    let mut failures = Vec::new();
    let mut cases = 0;
    for file in ["game_messages", "game_events", "world_object_messages"] {
        for case in &vectors::load_named("messages", file).cases {
            let class = s(&case.input["class"]);
            let ace = unhex(s(&case.output["hex"]));
            let ace = retail_ruled(class, &ace).unwrap_or(ace);
            let built = build_vector_case(file, case);
            if built.data != ace {
                failures.push(format!(
                    "{class}: the builder writes {}, ACE {}",
                    hex(&built.data),
                    hex(&ace)
                ));
                continue;
            }
            failures.extend(check(class, codec(class), body(&ace)));
            cases += 1;
        }
    }
    assert!(cases > 250, "only {cases} message cases");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A `PackableList` of `T`.
macro_rules! list_of {
    ($t:ty) => {
        |body: &[u8]| -> Result<Vec<u8>, String> {
            let mut r = Reader::body(body);
            let m = r
                .packed_list(<$t>::read)
                .map_err(|e| format!("read: {e}"))?;
            if r.remaining() > 0 {
                return Err(format!("{} bytes left", r.remaining()));
            }
            let mut w = Writer::body();
            w.packed_list(&m, |w, v| Written::written(v.write(w)))
                .map_err(|e| format!("write: {e}"))?;
            Ok(w.into_inner())
        }
    };
}

/// `Reader::pstring` as a record, for `list_of!`.
struct PString(String);

impl PString {
    fn read(r: &mut Reader<'_>) -> Result<Self, dp::MessageError> {
        r.pstring().map(Self)
    }
    fn write(&self, w: &mut Writer) -> Result<(), dp::MessageError> {
        w.pstring(&self.0)
    }
}

/// Two `u16`s: a layered spell id (spell, layer), for `list_of!`.
struct U16Pair(u16, u16);

impl U16Pair {
    fn read(r: &mut Reader<'_>) -> Result<Self, dp::MessageError> {
        Ok(Self(r.u16()?, r.u16()?))
    }
    fn write(&self, w: &mut Writer) {
        w.u16(self.0);
        w.u16(self.1);
    }
}

/// ACE's officer table: a `PHashTable<uint, uint>`.
fn officers(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut r = Reader::body(body);
    let t = r
        .phash(|r| Ok((r.u32()?, r.u32()?)))
        .map_err(|e| format!("read: {e}"))?;
    if r.remaining() > 0 {
        return Err(format!("{} bytes left", r.remaining()));
    }
    let mut w = Writer::body();
    w.phash(&t, |w, k, v| {
        w.u32(*k);
        w.u32(*v);
        Ok(())
    })
    .map_err(|e| format!("write: {e}"))?;
    Ok(w.into_inner())
}

/// The dereth-protocol codec of one `serialization_structures` kind.
fn structure_codec(kind: &str) -> Option<RoundTrip> {
    use dp::types::{appraisal::*, qualities::*, space::*, weeniedesc::RestrictionDb};
    Some(match kind {
        "allegiance_data_null" => sub_record!(dp::social::AllegianceData),
        "allegiance_profile_null" => sub_record!(dp::social::AllegianceProfile),
        "contract_tracker" => sub_record!(dp::social::ContractTracker),
        "fellowship_lock_data" => sub_record!(dp::social::FellowshipLock),
        "fellowship_locks" => sub_record!(dp::social::FellowshipLocks),
        "squelch_info" => sub_record!(dp::comms::SquelchInfo),
        "squelch_db" => sub_record!(dp::comms::SquelchDb),
        "salvage_result" => sub_record!(dp::items::SalvageResult),
        "guest_info" => sub_record!(dp::trade::GuestInfo),
        "house_access" => sub_record!(dp::trade::Har),
        "house_profile" => sub_record!(dp::trade::HouseProfile),
        "house_payment_list" => list_of!(dp::trade::HousePayment),
        "house_data" => rt::<dp::trade::HouseDataMessage>,
        "page_data" => sub_record!(dp::trade::PageData),
        "chess_move_data" => sub_record!(dp::trade::GameMoveData),
        "enchantment_list" => list_of!(Enchantment),
        "shortcut_list" => list_of!(dp::login::ShortCutData),
        "layered_spell_list" | "registry_list" => list_of!(U16Pair),
        "strings" => list_of!(PString),
        "officers" => officers,
        "position" => sub_record!(PositionWire),
        "origin" => sub_record!(Origin),
        "position_pack" => sub_record!(dp::movement::PositionPack),
        "armor_profile" => sub_record!(ArmorProfile),
        "creature_profile" => sub_record!(CreatureAppraisalProfile),
        "weapon_profile" => sub_record!(WeaponProfile),
        "hook_profile" => sub_record!(HookAppraisalProfile),
        "appraise_info_full" | "appraise_info_empty" => sub_record!(AppraisalProfile),
        "enchantment" => sub_record!(Enchantment),
        "enchantment_registry" => sub_record!(EnchantmentRegistry),
        "restriction_db" => sub_record!(RestrictionDb),
        _ => return None,
    })
}

/// Structure kinds with no dereth-protocol record of their own: `armor_level` is nine dwords inside the
/// appraisal profile (checked there).
const NO_RECORD: &[&str] = &["armor_level"];

fn world_object_codec(key: &str) -> RoundTrip {
    match key {
        "create" | "create_admin" => rt::<dp::objects::ItemCreateObject>,
        "update" => rt::<dp::objects::ItemUpdateObject>,
        "obj_desc" => rt::<dp::objects::ItemObjDescEvent>,
        "game_data" => sub_record!(dp::types::PublicWeenieDesc),
        other => panic!("no codec for {other}"),
    }
}

const WORLD_OBJECT_MESSAGES: [&str; 4] = ["create", "create_admin", "update", "obj_desc"];

#[test]
fn dereth_protocol_reads_and_rewrites_aces_serialised_objects_and_structures() {
    let mut failures = Vec::new();
    let mut cases = 0;
    for case in &vectors::load_named("messages", "serialization_world_objects").cases {
        let guid = case.input["object"]["guid"].as_u64().unwrap();
        for key in WORLD_OBJECT_MESSAGES {
            let ace = unhex(s(&case.output[key]));
            let ace = retail_ruled_object(key, &ace).unwrap_or(ace);
            failures.extend(check(
                &format!("world_objects.{key}.{guid:08X}"),
                Some(world_object_codec(key)),
                body(&ace),
            ));
            cases += 1;
        }
    }
    for case in &vectors::load_named("messages", "serialization_appraise").cases {
        let ace = unhex(s(&case.output["hex"]));
        failures.extend(check(
            "appraise",
            Some(sub_record!(dp::types::appraisal::AppraisalProfile)),
            &ace,
        ));
        cases += 1;
    }
    for case in &vectors::load_named("messages", "serialization_motion").cases {
        let ace = unhex(s(&case.output["hex"]));
        let f: RoundTrip = if case.input["header"] == Value::Bool(false) {
            sub_record!(dp::movement::MovementBody)
        } else {
            sub_record!(dp::movement::MovementBuffer)
        };
        failures.extend(check("motion", Some(f), &ace));
        cases += 1;
    }
    for case in &vectors::load_named("messages", "serialization_structures").cases {
        let kind = s(&case.input["kind"]);
        if NO_RECORD.contains(&kind) {
            continue;
        }
        let ace = unhex(s(&case.output["hex"]));
        let ace = retail_ruled_structure(kind, &ace).unwrap_or(ace);
        failures.extend(check(
            &format!("structures.{kind}"),
            structure_codec(kind),
            &ace,
        ));
        cases += 1;
    }
    assert!(cases > 170, "only {cases} serialisation cases");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_known_difference_names_a_vector_case() {
    let mut keys: Vec<String> = Vec::new();
    for file in ["game_messages", "game_events", "world_object_messages"] {
        keys.extend(
            vectors::load_named("messages", file)
                .cases
                .iter()
                .map(|c| s(&c.input["class"]).to_owned()),
        );
    }
    for case in &vectors::load_named("messages", "serialization_world_objects").cases {
        let guid = case.input["object"]["guid"].as_u64().unwrap();
        keys.extend(WORLD_OBJECT_MESSAGES.map(|k| format!("world_objects.{k}.{guid:08X}")));
    }
    for case in &vectors::load_named("messages", "serialization_structures").cases {
        keys.push(format!("structures.{}", s(&case.input["kind"])));
    }
    for (k, _, why) in KNOWN {
        assert!(
            keys.iter().any(|x| x == k),
            "KNOWN row {k} ({why}) names no vector case"
        );
    }
}

/// V236: the server writes a known-type DataID in retail's short form, so the two objects whose icon
/// ids have a low word in 0x4000..=0x7FFF come out four bytes longer than ACE recorded (the same
/// ids: both forms read to one value); nothing else ACE recorded changes. (0x80000003's icon,
/// 0x7FFF, lies below the icon base, so V236/V237 then sends it as none: `serialization.rs`.)
#[test]
fn the_server_writes_two_objects_icons_in_retails_short_form() {
    let mut ruled = Vec::new();
    for case in &vectors::load_named("messages", "serialization_world_objects").cases {
        let guid = case.input["object"]["guid"].as_u64().unwrap();
        for key in WORLD_OBJECT_MESSAGES.iter().chain(&["game_data"]) {
            let ace = unhex(s(&case.output[key]));
            let Some(server) = retail_ruled_object(key, &ace) else {
                continue;
            };
            let f = world_object_codec(key);
            let read = |data: &[u8]| at_short_limit(ACE_KNOWN_TYPE_SHORT_LIMIT, || f(body(data)));
            assert_eq!(
                read(&server),
                read(&ace),
                "{key}.{guid:08X}: the same fields"
            );
            ruled.push(format!("{key}.{guid:08X} +{}", server.len() - ace.len()));
        }
    }
    assert_eq!(
        ruled,
        [
            "create.80000002 +4",
            "create_admin.80000002 +4",
            "update.80000002 +4",
            "game_data.80000002 +4",
            "create.80000003 +4",
            "create_admin.80000003 +4",
            "update.80000003 +4",
            "game_data.80000003 +4",
        ]
    );
    let mut messages = Vec::new();
    for file in ["game_messages", "game_events", "world_object_messages"] {
        for case in &vectors::load_named("messages", file).cases {
            let class = s(&case.input["class"]);
            let ace = unhex(s(&case.output["hex"]));
            if codec(class)
                .is_some_and(|f| retail_packed(f, &ace, ace.len() - body(&ace).len()).is_some())
            {
                messages.push(class.to_owned());
            }
        }
    }
    assert!(
        messages.is_empty(),
        "message vectors V236 changes: {messages:?}"
    );
}
