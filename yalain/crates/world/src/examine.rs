//! Selection and examination: what the server tells a client about the object its player selects
//! and the objects it examines.
//!
//! **Selection.** When the client selects a player, a creature or a person, the server answers
//! with its selection information: its health and vigor, current and greatest, and what
//! player-killer damage it has taken. Selecting any other object, or nothing, is not answered.
//!
//! **Examination.** Each request about an object or an attackable target is answered with the
//! object's profile ([`crate::profile`]); a request to stop is not answered. While the player
//! examines it, the object is looked at again every 3.5 s and its profile sent again when it has
//! changed (a creature's or a player's vitals, a stack's count). A request about another object,
//! or a request to stop sent later than the request it stops, ends that; a request to stop sent at
//! the same moment as a request (the client stopping the earlier examination as it starts the
//! next) does not.

use auberean_protocol::types::Qualities;
use auberean_protocol::wire::Field;
use auberean_protocol::wire::Reader;
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::native_object::NativeObject;

use crate::graph::Builder;
use crate::player::quality;
use crate::session;

/// The client functions selection and examination answer with.
pub mod function {
    /// The selected object's vitals.
    pub const SELECTION_INFO: u32 = 0x8212_002C;
}

/// The native type of a selection's information.
pub const SELECTION_INFO_TYPE: u16 = 171;

/// A selection's information of an object with vitals.
pub const WITH_VITALS: u32 = 0x4000_0001;

/// An object's vitals as selection shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Vitals {
    pub health: u32,
    pub max_health: u32,
    pub vigor: u32,
    pub max_vigor: u32,
    /// Player-killer damage taken, to health and to vigor.
    pub pk_damage: u32,
    pub pk_vigor_loss: u32,
}

impl Vitals {
    /// A player's vitals, from its qualities.
    #[must_use]
    pub fn of_qualities(bytes: &[u8]) -> Option<Self> {
        let q = Qualities::read(&mut Reader::new(bytes)).ok()?;
        let int = |k: u32| q.ints.get(k).copied().map(i32::cast_unsigned);
        Some(Self {
            health: int(quality::HEALTH)?,
            max_health: int(quality::HEALTH_MAX)?,
            vigor: int(quality::VIGOR).unwrap_or(0),
            max_vigor: int(quality::VIGOR_MAX).unwrap_or(0),
            pk_damage: 0,
            pk_vigor_loss: 0,
        })
    }
}

/// The selection information's packed form: none, or the vitals in the order the client reads
/// them.
#[must_use]
pub fn selection_bytes(v: Option<Vitals>) -> Vec<u8> {
    let words: Vec<u32> = match v {
        None => vec![0],
        Some(v) => vec![
            WITH_VITALS,
            v.health,
            v.pk_damage,
            v.max_health,
            v.vigor,
            v.pk_vigor_loss,
            v.max_vigor,
        ],
    };
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// The answer to a selection: the selected object and its information (packed last parameter
/// first: the information, then the object).
#[must_use]
pub fn selection_info(selected: u64, v: Option<Vitals>) -> ServerMessage {
    let mut b = Builder::new();
    b.native(
        SELECTION_INFO_TYPE,
        NativeObject::Packed {
            ty: SELECTION_INFO_TYPE.into(),
            bytes: selection_bytes(v),
        },
    );
    session::call(
        function::SELECTION_INFO,
        &[Arg::Object(b.graph()), Arg::LongInt(selected)],
    )
    .expect("a selection packs")
}

/// The health and vigor of an object whose template names none, a person most often (2017: 1,677 of
/// 1,683 selections of such objects, of 356 templates).
const UNNAMED_HEALTH: i32 = 30;
const UNNAMED_VIGOR: i32 = 130;

/// The vitals of an object of the world that has them (a creature or a person: its template's
/// greatest health and vigor, untouched).
#[must_use]
pub fn content_vitals(o: &yalain_content::ContentObject) -> Option<Vitals> {
    if o.kind == yalain_content::Kind::Static {
        return None;
    }
    let h = o.facts.max_health.unwrap_or(UNNAMED_HEALTH).cast_unsigned();
    let v = o.facts.max_vigor.unwrap_or(UNNAMED_VIGOR).cast_unsigned();
    Some(Vitals {
        health: h,
        max_health: h,
        vigor: v,
        max_vigor: v,
        pk_damage: 0,
        pk_vigor_loss: 0,
    })
}

/// A player's level: its level quality, else 1.
#[must_use]
pub fn level_of(qualities: &[u8]) -> i32 {
    Qualities::read(&mut Reader::new(qualities))
        .ok()
        .and_then(|q| q.ints.get(LEVEL).copied())
        .unwrap_or(1)
}

/// The quality holding a player's level.
pub const LEVEL: u32 = 302;

/// The combat mode an examiner without a weapon is in, and the accuracy it reports.
pub const PEACE_MODE: u32 = 0x4000_0001;
pub const NO_ACCURACY: u32 = 2;

/// How often what a player examines is looked at again, in seconds.
pub const AGAIN_SECONDS: f64 = 3.5;

/// What a player is examining: the request as it was asked, its number (a newer request's is
/// higher), when it was asked, and the profile last sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Examining {
    pub request: Vec<u8>,
    pub number: u32,
    pub asked_at: f64,
    pub sent: Vec<u8>,
}
