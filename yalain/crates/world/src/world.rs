//! The world connection: a character entering, standing and moving in the world, and leaving.
//!
//! **Entering.** As the client's world connection comes up (or, without a referral, as the
//! client asks to enter on its login connection), the server sends, in this order: the player's
//! object, the message that makes it the client's player, its description (its qualities), the
//! portal effect on it, a freeze of its movement, the end of the effect it left with, the session
//! start, the welcome text, then the objects of the world around it. Private messages are
//! numbered from 1 on each world connection; the client takes them in that order.
//!
//! **The scene.** When the client says it has drawn the world, the server frees its movement and
//! ends the portal effect.
//!
//! **Moving.** The client reports where its player is; the server takes it (the client decides
//! within what a body can do), sends the player its own position back, and creates the objects that
//! come into range as it crosses landblocks. Turning the head is sent back the same way.
//!
//! **A place too far.** A report that goes further than a body could have since the last ones takes
//! the player nowhere: the server moves its force-position stamp on and tells the player where it
//! stands, and its client goes back there (2017: 608 such corrections in 1,806,692 reports, the next
//! report from the corrected place, at its stamp, in 515). Reports that have not heard of the new
//! stamp are not taken. A land cell its point does not lie in is no place at all.
//!
//! **Options.** The client sends its options as they change; the server keeps them for the next
//! session start, up to what a character keeps ([`crate::options`]).
//!
//! **Logging off.** A character whose player logs off stays in the world, its movement frozen,
//! for about nineteen seconds: it glows at once (the portal effect, for all who see it), plays its
//! leaving behavior at 15.2 s, and at 18.9 s goes into portal space, its auras and the effects that
//! end at logout taken off, its own look shown; a moment later it leaves the world (others see it
//! go, its pets forgotten with it) and the login connection is told it is out, with the character
//! list.
//! It may be fought, and its pets fight on, until then.
//!
//! **Leaving.** The character's place, qualities and options are kept.

use dereth_primitives::num::math;
use std::collections::BTreeMap;

use auberean_protocol::login::CharacterRefused;
use auberean_protocol::physics::{position_flags, Fx, ObjectPosition, Word};
use auberean_protocol::types::{ObjectStamp, Qualities, VisualDesc};
use auberean_protocol::wire::{Field, Reader, Writer};
use auberean_protocol::{queue, ClientBlob, ClientMessage, ServerMessage};
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::{Store, StoredPosition, WorldState};

use crate::chargen::BODY_ENTITIES;
use crate::options::{Options, Pairs};
use crate::player;
use crate::presence::{self, Seen};
use crate::session::{self, request, SessionStart};
use crate::uploads::{self, Value};
use crate::{Clock, Server};

/// The effect a character arrives in the world with, and the one it leaves the portal with.
pub const ARRIVAL_FX: u32 = 0x4100_0010;
pub const PORTAL_FX: u32 = 0x4100_0002;

/// The id the server gives the content pack's objects that have none of their own: the
/// runtime objects' range, numbered from 1 in pack order.
pub const CONTENT_ID_BASE: u64 = 0x8130_0000_0000_0000;

/// The position flag of an object touching the ground: the server sends its players' places so.
pub const ON_GROUND: u32 = 0x4;

/// A character in the world, by its world connection.
#[derive(Debug, Clone)]
pub struct Player {
    pub character: u64,
    pub login: ConnId,
    pub name: String,
    pub race: u32,
    pub sex: u32,
    /// It entered as an admin character: of the admin template, from an admin's account. Admin
    /// calls also need the account's flag at the time of the call.
    pub admin_character: bool,
    /// What an admin's commands have set on it, while it is in the world.
    pub admin: crate::admin_calls::AdminState,
    /// How it looks, as the character list shows it.
    pub look: VisualDesc,
    /// The instance stamp of its object.
    pub instance: u16,
    /// The last visual-ordering stamp its object's visual events used.
    pub visual: u16,
    /// The last position stamp sent.
    pub position_stamp: u16,
    /// The teleport stamp: moved on each time the server puts the player somewhere.
    pub teleport_stamp: u16,
    /// The force-position stamp: moved on each time the server refuses a place the player reported.
    pub force_position_stamp: u16,
    /// How far its reports may yet take it.
    pub travel: Travel,
    /// When it was last told where it stands after a place it reported was refused.
    pub told_back: f64,
    pub position: StoredPosition,
    /// Its heading and asked motion as last reported.
    pub heading_motion: u32,
    pub position_stream: u32,
    pub head_stream: u32,
    /// When its place last brought its view up to date, and whether its place has moved it since.
    pub view_brought: f64,
    pub view_owed: bool,
    /// The objects and players its client has been told of, by id.
    pub known: BTreeMap<u64, Seen>,
    pub options: Options,
    pub qualities: Vec<u8>,
    /// The client has drawn the world.
    pub scene_drawn: bool,
    /// The object it has selected (0 for none).
    pub selection: u64,
    /// An admin call of its was refused and logged this stay.
    pub refused_admin_call: bool,
    /// An admin command it typed was refused and logged this stay.
    pub refused_admin_line: bool,
    /// When it entered the world this time, on the local clock.
    pub entered_at: f64,
    /// The stamp of its last movement order.
    pub move_to_stamp: u16,
    /// A use it is being walked toward: the use and the reach it needs.
    pub walking_to: Option<(crate::places::Use, f32)>,
    /// An item it is giving to a person or an object.
    pub giving: Option<crate::gives::Giving>,
    /// The object it follows.
    pub following: Option<u64>,
    /// Where it last reported itself at rest while walked toward a use.
    pub walk_rest: Option<StoredPosition>,
    /// Its trip through portal space, while it is under way.
    pub trip: Option<crate::places::Trip>,
    /// The places it keeps, by kind.
    pub places: Vec<(u32, StoredPosition)>,
    /// What it holds and has done in play.
    pub play: crate::state::PlayState,
    /// The store it is in: the storekeeper, the store, the first sale of the page shown.
    pub shopping: Option<(u64, u32, u32)>,
    /// The rows of market pages its connection may yet have ([`crate::market`]).
    pub page_rows: Option<crate::call_limit::Bucket>,
    /// Its attack run, while it attacks.
    pub attacking: Option<crate::combat::Run>,
    /// Its death, until it is resurrected.
    pub dying: Option<crate::death::Dying>,
    /// The container it has open.
    pub open_container: Option<u64>,
    /// When its vitals last regenerated, and when they next do (world seconds; 0 before its first
    /// step in the world).
    pub last_regen: f64,
    pub next_regen: f64,
    /// Whether it fought or was hurt since its vitals last regenerated.
    pub regen_active: bool,
    /// Its health when the regeneration last looked (a lower health since is a hurt).
    pub regen_health: i32,
    /// The recipe it is making, while it makes one.
    pub making: Option<crate::craft::Making>,
    /// Its specials' missiles in flight.
    pub flying: Vec<crate::skill_use::Flying>,
    /// Its lifestone protection, while it lasts.
    pub protected: Option<crate::effects::Protected>,
    /// The skill it is using, while the use runs.
    pub using: Option<crate::skill_use::Using>,
    /// A skill's use waiting for the running one to end.
    pub queued: Option<crate::skill_use::Request>,
    /// A special waiting for its attack run's next swing.
    pub in_run: Option<crate::skill_use::Request>,
    /// How fast it may talk, once it has, and when it was last told it is past that.
    pub talk: Option<(crate::call_limit::Bucket, Option<f64>)>,
    /// When it last used each skill (game time), from which the skill's recovery counts.
    pub last_used: std::collections::HashMap<u32, f64>,
    /// Another player it is walked to, and what for.
    pub approaching: Option<(u64, crate::social::Approach)>,
    /// The story shard's protection on it (its effect's id), until it ends.
    pub shard_protection: Option<u32>,
    /// The mount it rides.
    pub riding: Option<crate::mounts::Riding>,
    /// It logged off and is leaving the world.
    pub logging_off: bool,
    /// Its client went away without logging off: it leaves at its leaving behavior, without the
    /// trip into portal space.
    pub dropped: bool,
    /// A skill it is untraining, until it has earned the skill's experience again.
    pub untraining: Option<crate::skills::Untraining>,
    /// The conflict status of where it is, as its client was last told it.
    pub conflict: u32,
    /// When it last asked to change its account's password (the transport's seconds).
    pub passwd_at: Option<f64>,
}

impl Player {
    /// Its name as the world shows it: with the words an admin put after it, if any.
    #[must_use]
    pub fn shown_name(&self) -> String {
        if self.play.suffix.is_empty() {
            self.name.clone()
        } else {
            format!("{} {}", self.name, self.play.suffix)
        }
    }

    /// Its object, with its instance stamp.
    #[must_use]
    pub const fn stamp(&self) -> ObjectStamp {
        ObjectStamp {
            id: self.character,
            instance_stamp: self.instance,
            other_stamp: 0,
        }
    }

    /// The stamps its object is made with: position, teleport, force-position, move-to.
    #[must_use]
    pub const fn physics_stamps(&self) -> [u16; 4] {
        [
            self.position_stamp,
            self.teleport_stamp,
            self.force_position_stamp,
            self.move_to_stamp,
        ]
    }

    /// Its place as the wire sends it, with the next position stamp.
    pub fn next_position(&mut self, time: f64, flags: u32) -> ObjectPosition {
        self.position_stamp = self.position_stamp.wrapping_add(1);
        ObjectPosition {
            sender: self.stamp(),
            time,
            cell: self.position.cell,
            origin: auberean_primitives::Vec3::new(
                self.position.origin[0],
                self.position.origin[1],
                self.position.origin[2],
            ),
            heading_motion: self.heading_motion,
            flags,
            position_stamp: self.position_stamp,
            force_position_stamp: self.force_position_stamp,
            teleport_stamp: self.teleport_stamp,
            ..ObjectPosition::default()
        }
    }
}

fn bytes_of<F: Field>(f: &F) -> Vec<u8> {
    let mut w = Writer::default();
    f.write(&mut w);
    w.finish()
}

fn send(net: &mut ServerNet, conn: ConnId, kind: BlobKind, m: &ServerMessage) {
    let bytes = m.encode();
    tracing::debug!(
        ?conn,
        kind = kind.0,
        len = bytes.len(),
        "sent {:#x}",
        m.message_type()
    );
    net.send(conn, queue::EVENT, kind, bytes);
}

/// A quaternion turning by a heading (degrees times 255/360, in a position's top byte): the
/// world turns by the heading's opposite about its up axis.
#[must_use]
pub fn rotation_of_heading(heading: u8) -> [f32; 4] {
    let a = -(f32::from(heading) * 360.0 / 255.0).to_radians() / 2.0;
    [math::cosf(a), 0.0, 0.0, math::sinf(a)]
}

/// A rotation's heading, as a position's top byte carries it.
#[must_use]
pub fn heading_of_rotation(r: [f32; 4]) -> u8 {
    let angle = 2.0 * math::atan2f(r[3], r[0]);
    let deg = (-angle.to_degrees()).rem_euclid(360.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (deg * 255.0 / 360.0).round() as u32;
    u8::try_from(b % 256).unwrap_or(0)
}

/// The place a new character of a race starts at: its start area's copy that holds the most of
/// the world's objects (the copy the pack was made for), else its first.
fn start_place<S: Store>(s: &Server<S>, race: u32, chosen: u32) -> Option<StoredPosition> {
    let area = s.start.area_for(race, chosen)?;
    area.places
        .iter()
        .enumerate()
        .max_by_key(|(i, p)| {
            let n = s
                .content
                .objects
                .iter()
                .filter(|o| o.landblock() == (p.cell >> 16) as u16)
                .count();
            (n, usize::MAX - i)
        })
        .map(|(_, p)| *p)
}

/// What a character is in the world, made the first time it enters.
pub(crate) fn first_state<S: Store>(s: &Server<S>, c: &yalain_store::Character) -> WorldState {
    let body = BODY_ENTITIES
        .iter()
        .find(|(r, x, _)| *r == c.race && *x == c.sex)
        .map_or(0, |(_, _, e)| *e);
    let skills = s.start.skills.get(&c.race).cloned().unwrap_or_default();
    WorldState {
        position: start_place(s, c.race, c.start_area),
        instances: 0,
        qualities: bytes_of(&player::new_qualities(&c.name, c.race, c.sex, body)),
        options: Options::new_character(&skills).to_packed(),
        places: Vec::new(),
        play: Vec::new(),
    }
}

/// What a character is kept as, for its entry; the reason its client is refused when it cannot be
/// read. A character whose kept state cannot be read does not enter: played on a new state, its next
/// save would write over what it had. The bytes stay where they are in the store, and the operator is
/// told which.
pub(crate) fn kept_for_entry<S: Store>(
    s: &mut Server<S>,
    c: &yalain_store::Character,
) -> Result<(WorldState, crate::state::PlayState), u32> {
    let id = format_args!("{:#x}", c.id);
    let st = match crate::saves::state_of(s, c.id) {
        Ok(st) => st.unwrap_or_default(),
        Err(e) => {
            tracing::error!(
                id = %id,
                "{}: the store could not read this character, so it may not enter: {e}",
                c.name
            );
            return Err(CharacterRefused::ERROR);
        }
    };
    let damaged = |what: &str, e: &str| {
        tracing::error!(
            id = %id,
            "{}: its kept {what} does not read, so it may not enter: {e}. Nothing is changed: its \
             bytes stay in the database, in the character's rows of world_state and play_state",
            c.name
        );
        CharacterRefused::CORRUPT_CHARACTER
    };
    let play = crate::state::PlayState::from_bytes(&st.play).map_err(|e| {
        if crate::state::kept_by_a_newer_build(&st.play) {
            tracing::error!(
                id = %id,
                "{}: a newer Yalain kept this character, so it may not enter this one: {e}. Nothing \
                 is changed: run that Yalain, or put back a backup kept before its upgrade",
                c.name
            );
            return CharacterRefused::OLD_CHARACTER;
        }
        damaged("play state", &e)
    })?;
    if !st.qualities.is_empty() {
        Qualities::read(&mut Reader::new(&st.qualities))
            .map_err(|e| damaged("qualities", &e.to_string()))?;
    }
    if !st.options.is_empty() {
        Options::from_packed(&st.options).map_err(|e| damaged("options", &e))?;
    }
    Ok((st, play))
}

/// A character enters the world on a connection (its world connection, or its login connection
/// when the server sends no referral). A character that may not is not touched, and the reason its
/// client is to be told comes back.
///
/// # Errors
/// The reason to refuse it with: its state cannot be read.
pub fn enter<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    login: ConnId,
    character: u64,
    clock: Clock,
) -> Result<(), u32> {
    let c = match s.store.character(character) {
        Ok(Some(c)) => c,
        Ok(None) => {
            s.unhandled
                .note("entering with a character the store has not");
            return Err(CharacterRefused::NOT_OWNED);
        }
        Err(e) => {
            tracing::error!(
                id = format_args!("{character:#x}"),
                "the store could not read this character, so it may not enter: {e}"
            );
            return Err(CharacterRefused::ERROR);
        }
    };
    let (mut st, play) = kept_for_entry(s, &c)?;
    let fresh = first_state(s, &c);
    if st.qualities.is_empty() {
        st.qualities = fresh.qualities;
    }
    if st.options.is_empty() {
        st.options = fresh.options;
    }
    if st.position.is_none() {
        st.position = fresh.position;
    }
    let Some(position) = st.position else {
        tracing::error!(
            id = format_args!("{character:#x}"),
            race = c.race,
            "this character has no place to start: its race has no start area in this world's data, \
             so it may not enter"
        );
        return Err(CharacterRefused::COULD_NOT_PLACE);
    };
    // One that left the world dead enters where it returns to, and comes back a moment later.
    let dead = crate::examine::Vitals::of_qualities(&st.qualities).is_some_and(|v| v.health == 0);
    let position = if dead {
        crate::places::resurrection_place_of(s, &st.places, position)
    } else {
        position
    };
    st.position = Some(position);
    // Never in the world before (one the operator made with a build holds what it was given, and
    // is new all the same).
    let first_entry = st.instances == 0;
    st.instances = st.instances.wrapping_add(1).max(1);
    let new_character = st.play.is_empty() || first_entry;
    let conflict = crate::kingdoms::status_at(&s.content.conflict, position.cell);
    crate::kingdoms::entering(&mut st.qualities, conflict, new_character);
    s.saves.hold(character, st.clone());
    let mut options = Options::from_packed(&st.options).map_err(|e| {
        tracing::error!(
            id = format_args!("{character:#x}"),
            "{}: a new character's options do not read: {e}",
            c.name
        );
        CharacterRefused::CORRUPT_CHARACTER
    })?;
    let left_out = options.keep_within_bounds();
    if left_out.any() {
        tracing::warn!(
            id = format_args!("{character:#x}"),
            aliases = left_out.aliases,
            parts = left_out.parts,
            "{}: options past what a character keeps were left out as it entered",
            c.name
        );
    }
    // The template follows the account's flag at every entry: an admin character of an account no
    // longer an admin's enters as a player.
    let admin_character = c.entity_did == player::ADMIN_ENTITY
        && s.sessions.get(&login).is_some_and(|x| x.account.admin);
    player::set_template(&mut st.qualities, admin_character);
    let qualities = Qualities::read(&mut Reader::new(&st.qualities)).unwrap_or_default();
    let look = VisualDesc::read(&mut Reader::new(&c.look)).unwrap_or_default();
    // A connection that carried a character before (no referral) numbers its private messages
    // from 1 again.
    net.restart_stamps(world, queue::EVENT, BlobKind::PRIVATE);
    let mut p = Player {
        character,
        login,
        name: c.name.clone(),
        race: c.race,
        sex: c.sex,
        admin_character,
        admin: crate::admin_calls::AdminState::default(),
        look: look.clone(),
        instance: st.instances,
        visual: player::PLAYER_VISUAL_STAMP,
        position_stamp: 1,
        teleport_stamp: 0,
        force_position_stamp: 0,
        travel: Travel::default(),
        told_back: f64::NEG_INFINITY,
        view_brought: f64::NEG_INFINITY,
        view_owed: false,
        heading_motion: u32::from(heading_of_rotation(position.rotation)) << 24 | IDLE_MOTION,
        position,
        position_stream: net.new_stream(world).unwrap_or(0),
        head_stream: net.new_stream(world).unwrap_or(0),
        known: BTreeMap::new(),
        options,
        qualities: st.qualities.clone(),
        scene_drawn: false,
        selection: 0,
        refused_admin_call: false,
        refused_admin_line: false,
        entered_at: clock.local.0,
        move_to_stamp: 0,
        walking_to: None,
        giving: None,
        walk_rest: None,
        following: None,
        trip: None,
        places: st.places.clone(),
        play,
        shopping: None,
        page_rows: None,
        attacking: None,
        dying: None,
        open_container: None,
        last_regen: 0.0,
        next_regen: 0.0,
        regen_active: false,
        regen_health: 0,
        making: None,
        flying: Vec::new(),
        protected: None,
        passwd_at: None,
        using: None,
        queued: None,
        in_run: None,
        last_used: std::collections::HashMap::new(),
        talk: None,
        approaching: None,
        shard_protection: None,
        riding: None,
        logging_off: false,
        dropped: false,
        untraining: None,
        conflict,
    };
    p.admin.cloaked = p.admin_character && p.play.cloaked;
    let me = ObjectStamp {
        id: character,
        instance_stamp: p.instance,
        other_stamp: 0,
    };
    // It is drawn wearing what it wears.
    p.look = crate::equip::dressed(
        &look,
        &s.catalog,
        &p.play.items,
        &p.play.item_states,
        c.race,
        c.sex,
    );
    let at = player::placement(&position);
    let object = player::player_object(
        character,
        &p.shown_name(),
        c.sex,
        p.look.clone(),
        at,
        p.instance,
        &player::Marks::of(&qualities),
    );
    send(
        net,
        world,
        BlobKind::UNORDERED,
        &ServerMessage::CreateObject(object),
    );
    send(
        net,
        world,
        BlobKind::UNORDERED,
        &ServerMessage::PlayerObject(player::create_player(character)),
    );
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &ServerMessage::PlayerQualities(player::description(qualities)),
    );
    if new_character {
        crate::kingdoms::told_first(net, world, conflict);
    }
    p.visual = p.visual.wrapping_add(1);
    let fx = ServerMessage::DoFx(auberean_protocol::physics::ObjectEvent {
        sender: ObjectStamp {
            other_stamp: p.visual,
            ..me
        },
        action: Fx {
            fx: ARRIVAL_FX,
            scalar: 1.0,
        },
    });
    send(net, world, BlobKind::VISUAL, &fx);
    if new_character {
        // Its ethereality, the template's in its object, is set for where it stands.
        let want = crate::kingdoms::ethereality(conflict, crate::kingdoms::of(&p.qualities));
        if crate::kingdoms::kept_ethereality(&p.qualities) != want {
            crate::kingdoms::set_ethereality(&mut p.qualities, want);
            p.visual = p.visual.wrapping_add(1);
            let m = ServerMessage::UpdateIntVisual(auberean_protocol::quality::VisualQuality {
                object: ObjectStamp {
                    other_stamp: p.visual,
                    ..me
                },
                quality: crate::kingdoms::quality::ETHEREALITY,
                value: want,
            });
            send(net, world, BlobKind::VISUAL, &m);
        }
    }
    send(
        net,
        world,
        BlobKind::PRIVATE,
        &session::movement_frozen(true),
    );
    p.visual = p.visual.wrapping_add(1);
    let stop = ServerMessage::StopFx(auberean_protocol::physics::ObjectEvent {
        sender: ObjectStamp {
            other_stamp: p.visual,
            ..me
        },
        action: Word(PORTAL_FX),
    });
    send(net, world, BlobKind::VISUAL, &stop);
    if new_character {
        // A new character's pack: its race's starting weapons, from the character data.
        for template in crate::inventory::starting_pack(c.race) {
            if s.catalog.get(*template).is_some() {
                let id = s.things.new_id();
                let slot = u32::try_from(p.play.items.len()).unwrap_or(0);
                p.play.items.push(crate::state::Held {
                    id,
                    template: *template,
                    slot,
                    location: 0,
                    quantity: 1,
                });
            }
        }
    }
    let clashed = crate::inventory::fresh_ids_for_clashes(&mut s.things, &mut p.play, &c.name);
    // Whatever it wielded when it left, it still wields; the pack holds the rest.
    crate::inventory::close_up(&mut p.play.items);
    let mut pack: Vec<&crate::state::Held> =
        p.play.items.iter().filter(|h| h.location == 0).collect();
    pack.sort_by_key(|h| crate::inventory::pack_order(h.slot));
    let contents: Vec<u64> = pack.iter().map(|h| h.id).collect();
    let worn = crate::inventory::worn(
        &s.catalog,
        &p.play.items,
        &p.play.item_states,
        c.race,
        c.sex,
    );
    let game_now = net.game_time(clock.local);
    s.game_now = game_now;
    // A skill's wait does not run while the character is away, nor a quest's time: its last use,
    // and when each quest was given, move on by the time since the character was last kept.
    let away = if p.play.kept_at > 0.0 {
        (game_now - p.play.kept_at).max(0.0)
    } else {
        0.0
    };
    for (_, t) in &mut p.play.last_used {
        *t += away;
    }
    p.last_used = p.play.last_used.iter().copied().collect();
    #[allow(clippy::cast_precision_loss)]
    let game_epoch = game_now - clock.unix as f64;
    // The effects that ran out while the character was away are gone before its session starts.
    crate::effects::prune(&mut p.play, game_now);
    // So have the quests with no time left when it left; the others' time stood still.
    #[allow(clippy::cast_possible_truncation)]
    let away_s = away as i64;
    crate::quests::ran_out_before_leaving(&s.quests, &mut p.play.quests, clock.unix - away_s);
    crate::quests::time_stood_still(&mut p.play.quests, away_s, clock.unix);
    let effects = crate::effects::registry(s, &p);
    let start = SessionStart {
        server_faction: 0,
        local_faction: conflict,
        skills: crate::skills::held(
            &s.start.skills.get(&c.race).cloned().unwrap_or_default(),
            &p.play.skills,
        ),
        options: p.options.to_packed(),
        segments: crate::inventory::pack_segments(&p.play.items),
        money: crate::store::money(&p.qualities),
        contents,
        quests: crate::quests::list(&s.quests, &p.play.quests, game_now, game_epoch),
        credits: p.play.credits,
        hero_credits: p.play.hero_credits,
        effects,
        scenes: crate::scenes::registry(s, &p.play.scenes),
        worn,
        last_used: p.play.last_used.clone(),
    };
    match start.message() {
        Ok(m) => send(net, world, BlobKind::PRIVATE, &m),
        Err(e) => tracing::warn!("the session start does not pack: {e}"),
    }
    // The line that the friends list and tells are out of reach: only for an account the
    // world's friends service has no stream for (the client presents its ticket before the game
    // login, so one without a stream now has none to come), or when the world serves none.
    if s.friends_streams
        .as_ref()
        .is_none_or(|streams| !streams.contains(&c.account))
    {
        send(net, world, BlobKind::PRIVATE, &session::welcome());
    }
    s.players.insert(world, p);
    if dead {
        s.after(
            clock,
            crate::death::BACK_AFTER_ENTRY,
            crate::Timer::BackAfterEntry { world },
        );
    }
    if new_character {
        crate::scenes::first_entry(s, net, world);
    }
    crate::effects::enter(s, net, world, clock);
    crate::craft::enter(s, net, world, clock);
    crate::workshops::moved(s, net, world, clock);
    crate::quests::timers_at_entry(s, world, clock);
    // The items it holds: in its pack, and in its hands (held there, as it left them).
    let instance = s.players.get(&world).map_or(0, |p| p.instance);
    for id in crate::inventory::enter(s, world) {
        if let Some(t) = s.things.get(id) {
            let m =
                ServerMessage::CreateObject(crate::inventory::held_create(&s.catalog, t, instance));
            send(net, world, BlobKind::UNORDERED, &m);
        }
        if let Some(p) = s.players.get_mut(&world) {
            p.known.insert(
                id,
                presence::Seen {
                    position_stream: 0,
                    head_stream: 0,
                },
            );
        }
    }
    if new_character {
        crate::inventory::give(s, net, world, crate::inventory::GUIDE_BOOK, 1);
    }
    if clashed {
        keep_player(s, world);
    }
    crate::market::enter(s, net, world);
    presence::refresh(s, net, world);
    crate::chat::tell_allegiance_and_channels(s, net, world);
    crate::admin::entered(s, net, world);
    crate::admin_calls::entered(s, net, world);
    // The operator's message of the day, one notice (its lines in order), as Empyrean shows its
    // own at entry.
    if !s.cfg.motd.trim().is_empty() {
        crate::login::notice(net, world, &s.cfg.motd);
    }
    if s.saves.failing() {
        crate::login::notice(net, world, crate::saves::CANNOT_SAVE);
    }
    tracing::info!(
        target: crate::answers::TARGET,
        "{}",
        crate::answers::entered(
            &c.name,
            character,
            &format!(
                "in the world at {:08X} on {world:?}, {} objects known",
                position.cell,
                s.players.get(&world).map_or(0, |p| p.known.len())
            )
        )
    );
    Ok(())
}

/// Keep what a character is in the world. Its next periodic save is counted from now
/// ([`crate::saves`]).
fn keep<S: Store>(s: &mut Server<S>, p: &Player) {
    s.saves.kept(p.character);
    crate::admin::kept(s, p.character);
    let mut play = p.play.clone();
    play.last_used = p.last_used.iter().map(|(k, t)| (*k, *t)).collect();
    play.last_used.sort_by_key(|(k, _)| *k);
    play.kept_at = s.game_now;
    play.played_seconds = crate::saves::played_seconds(p, s.saves.now.0);
    play.cloaked = p.admin.cloaked;
    // What its items carry of their own (their instance stamps, their makers, their wear) is kept
    // with them.
    play.item_states = play
        .items
        .iter()
        .filter_map(|h| {
            // Not in the world yet (a save as its holder enters): as it was last kept.
            match s.things.get(h.id) {
                Some(t) => Some(crate::state::ItemState::of(t)),
                None => p.play.item_states.iter().find(|k| k.id == h.id).cloned(),
            }
        })
        .collect();
    let st = WorldState {
        position: Some(p.position),
        instances: p.instance,
        qualities: p.qualities.clone(),
        options: p.options.to_packed(),
        places: p.places.clone(),
        play: play.to_bytes(),
    };
    s.saves.hold(p.character, st);
}

/// Keep what the character on a world connection is in the world.
pub fn keep_player<S: Store>(s: &mut Server<S>, world: ConnId) {
    if let Some(p) = s.players.remove(&world) {
        keep(s, &p);
        s.players.insert(world, p);
    }
}

/// A character leaves the world (its connection is closing, or it logged off): every client that
/// knows it forgets it, and it is kept.
pub fn leave<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    // Its fellowship and its trade go on without it.
    if let Some(id) = s.players.get(&world).map(|p| p.character) {
        crate::admin_calls::rooms::leave_all(s, id);
        crate::fellowship::leave(s, net, id);
        crate::trade::left(s, net, id);
        crate::pets::owner_leaves(s, net, id);
        crate::plants::owner_leaves(s, net, id);
        crate::drops::left(s, world);
        crate::gives::left(s, net, world);
        crate::usable::chests::left(s, net, world);
        crate::usable::vaults::left(s, world);
    }
    if let Some(p) = s.players.remove(&world) {
        presence::depart(s, net, &p);
        keep(s, &p);
        crate::admin::left(s, p.character);
        crate::inventory::leave(s, &p.play.items);
        tracing::info!(
            target: crate::answers::TARGET,
            "{}",
            crate::answers::left(
                &p.name,
                p.character,
                &format!(
                    "left the world at {:08X} from {world:?}",
                    p.position.cell
                )
            )
        );
    }
}

/// The steps of a character's leaving after its player logged off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogOffStep {
    /// The leaving behavior plays.
    Behavior,
    /// Into portal space, its effects that end at logout off, its own look.
    Leaves,
    /// Out of the world, and the login told.
    Out,
}

/// A logging off: its behavior and its parameters' flag, and when each step comes after the
/// request (2017, 317 logoffs: the behavior a median 15.19 s after, p10 15.10, p90 15.46; portal
/// space and the look 18.87 and 18.91 s; others saw the character leave 0.06 to 0.1 s later, and
/// the login was told at 19.0 s).
pub mod log_off {
    pub const BEHAVIOR: u32 = 0x4100_0083;
    pub const BEHAVIOR_FLAG: u32 = 0x800;
    pub const BEHAVIOR_AFTER: f64 = 15.2;
    pub const LEAVES_AFTER: f64 = 18.9;
    pub const OUT_AFTER: f64 = 19.0;
    /// A character whose client went away leaves this long after its leaving behavior (2017, nine
    /// such leavings seen whole by others: 0.00 to 0.13 s).
    pub const DROPPED_OUT_AFTER_BEHAVIOR: f64 = 0.1;
}

/// A character's player logs off: it begins to leave (frozen, glowing), out of the world 18.9 s
/// on. Returns false for a connection with no character in the world.
pub fn log_off<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
) -> bool {
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    if p.logging_off {
        return true;
    }
    p.logging_off = true;
    begin_leaving(s, net, world, clock, false);
    true
}

/// A character's client went away while it was in the world (it said goodbye, or fell silent): it
/// lingers as a logging off does, frozen and glowing, and leaves at its leaving behavior, about
/// fifteen seconds on, without the trip into portal space. A character already logging off goes
/// on leaving as it was. Returns false for a connection with no character in the world.
pub fn dropped<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
) -> bool {
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    if p.logging_off {
        return true;
    }
    p.logging_off = true;
    p.dropped = true;
    begin_leaving(s, net, world, clock, true);
    true
}

/// The start of a leaving: frozen, the glow for all who see it, and the steps to come.
fn begin_leaving<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
    dropped: bool,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let id = p.character;
    let glow = ServerMessage::DoFx(crate::combat::player_event(
        p,
        Fx {
            fx: ARRIVAL_FX,
            scalar: 1.0,
        },
    ));
    crate::progress::private(net, world, &session::movement_frozen(true));
    crate::combat::to_all_seeing(s, net, id, &glow);
    s.after(
        clock,
        log_off::BEHAVIOR_AFTER,
        crate::Timer::LogOff {
            world,
            step: LogOffStep::Behavior,
        },
    );
    if dropped {
        s.after(
            clock,
            log_off::BEHAVIOR_AFTER + log_off::DROPPED_OUT_AFTER_BEHAVIOR,
            crate::Timer::LogOff {
                world,
                step: LogOffStep::Out,
            },
        );
        return;
    }
    s.after(
        clock,
        log_off::LEAVES_AFTER,
        crate::Timer::LogOff {
            world,
            step: LogOffStep::Leaves,
        },
    );
    s.after(
        clock,
        log_off::OUT_AFTER,
        crate::Timer::LogOff {
            world,
            step: LogOffStep::Out,
        },
    );
}

/// The next step of a character's leaving after its player logged off.
pub fn log_off_step<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: LogOffStep,
    clock: Clock,
) {
    if !s.players.get(&world).is_some_and(|p| p.logging_off) {
        return;
    }
    match step {
        LogOffStep::Behavior => {
            s.next_context = s.next_context.wrapping_add(1);
            let context = s.next_context;
            let Some(p) = s.players.get_mut(&world) else {
                return;
            };
            let id = p.character;
            let mut b = auberean_protocol::types::BehaviorParams {
                bits: auberean_protocol::types::BehaviorParams::BEHAVIOR
                    | log_off::BEHAVIOR_FLAG
                    | auberean_protocol::types::BehaviorParams::CONTEXT,
                behavior: log_off::BEHAVIOR,
                context,
                ..auberean_protocol::types::BehaviorParams::default()
            };
            // The word of flag 0x800 (the ninth of the flags followed by a word).
            b.words[8] = 1;
            let m = ServerMessage::PlayBehavior(crate::combat::player_event(p, b));
            crate::combat::to_all_seeing(s, net, id, &m);
        }
        LogOffStep::Leaves => {
            crate::progress::private(net, world, &crate::places::into_portal_space());
            crate::effects::auras_off(s, net, world);
            let gone: Vec<u32> = s.players.get(&world).map_or_else(Vec::new, |p| {
                p.play
                    .effects
                    .iter()
                    .filter(|e| e.flags & crate::effects::record_flag::LOGOUT != 0)
                    .map(|e| e.eid)
                    .collect()
            });
            for eid in gone {
                crate::effects::remove(s, net, world, eid);
            }
            let Some(p) = s.players.get_mut(&world) else {
                return;
            };
            let id = p.character;
            let look =
                ServerMessage::UpdateVisualDesc(crate::combat::player_event(p, p.look.clone()));
            crate::combat::to_all_seeing(s, net, id, &look);
        }
        LogOffStep::Out => {
            let Some(login) = s.players.get(&world).map(|p| p.login) else {
                return;
            };
            crate::login::logged_off(s, net, login, world, clock);
        }
    }
}

/// Is this a message of the world (rather than of the login connection)?
#[must_use]
pub fn is_world_message(m: &ClientMessage) -> bool {
    matches!(
        m,
        ClientMessage::ServerCall(_)
            | ClientMessage::Position(_)
            | ClientMessage::ForceCreate(_)
            | ClientMessage::LookAt(_)
            | ClientMessage::TurnHead(_)
            | ClientMessage::SceneRenderingComplete(_)
    )
}

/// The most a client's call may carry, in bytes. The largest of the 299,860 calls recorded in 2017
/// was 7,676 bytes; a message may be 8 MB, and what a call says is kept, or said to everyone near.
pub const MOST_CALL_BYTES: usize = 64 * 1024;

/// A message from a player's client on its world connection.
pub fn message<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    q: u16,
    bytes: &[u8],
    clock: Clock,
) {
    if !s.call_limits.allow(world, clock.local) {
        return;
    }
    let m = match ClientBlob::decode(q, bytes) {
        Ok(ClientBlob::Message(m)) => m,
        Ok(ClientBlob::Secure { queue, .. }) => {
            s.unhandled
                .note(format!("an encrypted message on queue {queue}"));
            return;
        }
        Err(e) => {
            s.unhandled
                .note(format!("a world message that does not read ({e})"));
            return;
        }
    };
    // Far behind in what it has been sent: its reports still count, what it asks for waits.
    if matches!(
        m,
        ClientMessage::ServerCall(_) | ClientMessage::ForceCreate(_)
    ) && net.large_waiting_on(world, queue::EVENT) >= crate::call_limit::MOST_BEHIND
    {
        return;
    }
    let Some(mut p) = s.players.remove(&world) else {
        s.unhandled.note(format!(
            "world message {:#x} before the character entered",
            m.message_type()
        ));
        return;
    };
    match m {
        ClientMessage::SceneRenderingComplete(_) => {
            if !p.scene_drawn {
                p.scene_drawn = true;
                send(
                    net,
                    world,
                    BlobKind::PRIVATE,
                    &session::movement_frozen(false),
                );
                p.visual = p.visual.wrapping_add(1);
                let stop = ServerMessage::StopFx(auberean_protocol::physics::ObjectEvent {
                    sender: ObjectStamp {
                        id: p.character,
                        instance_stamp: p.instance,
                        other_stamp: p.visual,
                    },
                    action: Word(ARRIVAL_FX),
                });
                let id = p.character;
                s.players.insert(world, p);
                // For all who see it, whose next visual message waits on this stamp (2017: 7,175
                // of the 8,186 of these a client took were about someone else).
                crate::combat::to_all_seeing(s, net, id, &stop);
                // The things of the other cells of an interior, held back until now.
                presence::refresh(s, net, world);
                // Arrived: the status of where it now is.
                return crate::kingdoms::check(s, net, world);
            }
        }
        ClientMessage::Position(c) => {
            s.players.insert(world, p);
            return moved(s, net, world, &c, clock);
        }
        ClientMessage::TurnHead(h) => {
            s.players.insert(world, p);
            return presence::tell_head(s, net, world, h, s.cfg.echo_positions);
        }
        ClientMessage::LookAt(t) => {
            s.players.insert(world, p);
            return presence::tell_look_at(s, net, world, t.0);
        }
        ClientMessage::ForceCreate(t) => {
            s.players.insert(world, p);
            return presence::force_create(s, net, world, t.0);
        }
        ClientMessage::ServerCall(call) if call.args.0.len() > MOST_CALL_BYTES => {
            s.unhandled
                .note(format!("a call of {} bytes", call.args.0.len()));
        }
        ClientMessage::ServerCall(call) => {
            if kept_call(&mut p, call.function.0, &call.args.0) {
                keep(s, &p);
            } else {
                s.players.insert(world, p);
                return crate::requests::call(s, net, world, &call, clock);
            }
        }
        other => s
            .unhandled
            .note(format!("world message {:#x}", other.message_type())),
    }
    s.players.insert(world, p);
}

/// The asked motion as a position's low 24 bits carry it: sideways, forward and vertical, each a
/// byte from 0 (full one way) to 255 (full the other), 127 for none.
#[must_use]
pub const fn motion(sideways: u8, forward: u8, vertical: u8) -> u32 {
    (sideways as u32) << 16 | (forward as u32) << 8 | vertical as u32
}

/// The asked motion of a player standing still: every motion byte at its middle.
pub const IDLE_MOTION: u32 = motion(127, 127, 127);

/// How many meters of a player's reports go unquestioned at once, and how fast more come, in meters
/// a second. Of 1,825,084 recorded reports, one needed more: 1,422 m after 252 s unheard. No 5 s of
/// recorded play covered more than 42 m a second.
pub const TRAVEL_HELD: f32 = 500.0;
pub const TRAVEL_SPEED: f32 = 60.0;
/// How long a client that has not heard where it stands goes before it is told again, in seconds.
pub const TOLD_BACK_AGAIN: f64 = 1.0;

/// How far a player's reports may yet take it, and when that was last reckoned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Travel {
    meters: f32,
    at: f64,
}

impl Default for Travel {
    fn default() -> Self {
        Self {
            meters: TRAVEL_HELD,
            at: f64::NEG_INFINITY,
        }
    }
}

impl Travel {
    /// A report would take the player this far, at this time: whether it may. A place it may not
    /// reach takes nothing from what it holds.
    #[allow(clippy::cast_possible_truncation)]
    pub fn spend(&mut self, meters: f32, now: f64) -> bool {
        let gained = (now - self.at).max(0.0) * f64::from(TRAVEL_SPEED);
        self.meters = (f64::from(self.meters) + gained).min(f64::from(TRAVEL_HELD)) as f32;
        self.at = now;
        if meters > self.meters {
            return false;
        }
        self.meters -= meters;
        true
    }
}

/// Is this land cell the one a point of its landblock lies in? All 1,503,957 reports recorded on the
/// land were.
#[must_use]
pub fn land_cell_holds(cell: u32, point: [f32; 2]) -> bool {
    let side = crate::spawn::BLOCK_SIDE;
    if !(0.0..=side).contains(&point[0]) || !(0.0..=side).contains(&point[1]) {
        return false;
    }
    let inside = |v: f32| v.min(side - 0.001);
    crate::spawn::outdoor(cell >> 16, inside(point[0]), inside(point[1])).0 == cell
}

fn moved<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    c: &auberean_protocol::physics::ClientPosition,
    clock: Clock,
) {
    // On the land, or within its side of a block: the 1,827,746 reports recorded came to 720 m.
    let on_land = [c.origin.x, c.origin.y, c.origin.z]
        .iter()
        .all(|v| v.abs() <= crate::spawn::LAND_SIDE);
    let on_its_cell = !s.cfg.holds_reports
        || presence::indoors(c.cell)
        || land_cell_holds(c.cell, [c.origin.x, c.origin.y]);
    if c.cell == 0 || !on_land || !on_its_cell {
        s.unhandled.note("a position that is no place");
        return;
    }
    let time = net.game_time(clock.local);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if c.teleport_stamp != p.teleport_stamp {
        // Reported from before the server last put the player somewhere: it is not there now.
        tracing::debug!(
            ?world,
            reported = c.teleport_stamp,
            current = p.teleport_stamp,
            "a place reported with an old teleport stamp"
        );
        return;
    }
    if c.force_position_stamp != p.force_position_stamp {
        // Reported from before the server last refused a place: the client has not heard yet, and
        // what told it may have been lost on the way.
        if clock.local.0 - p.told_back >= TOLD_BACK_AGAIN {
            p.told_back = clock.local.0;
            let m = p.next_position(time, ON_GROUND);
            let told = ServerMessage::Position(m).encode();
            net.send_ephemeral(world, queue::EVENT, p.position_stream, told);
        }
        return;
    }
    if s.cfg.holds_reports && !p.admin_character {
        let reported = StoredPosition {
            cell: c.cell,
            origin: [c.origin.x, c.origin.y, c.origin.z],
            rotation: p.position.rotation,
        };
        let (dx, dy, dz) = crate::ai::offset(&p.position, &reported);
        let far = (dx * dx + dy * dy + dz * dz).sqrt();
        if !p.travel.spend(far, clock.local.0) {
            tracing::debug!(
                ?world,
                far,
                "a place too far: the player is told where it stands"
            );
            p.force_position_stamp = p.force_position_stamp.wrapping_add(1);
            p.told_back = clock.local.0;
            let m = p.next_position(time, ON_GROUND);
            let told = ServerMessage::Position(m).encode();
            net.send_ephemeral(world, queue::EVENT, p.position_stream, told);
            return;
        }
    }
    if c.cell != p.position.cell {
        tracing::debug!(
            ?world,
            cell = format_args!("{:08X}", c.cell),
            "into a new cell"
        );
    }
    let before = p.position;
    #[allow(clippy::cast_possible_truncation)]
    let heading = (c.heading_motion >> 24) as u8;
    p.position = StoredPosition {
        cell: c.cell,
        origin: [c.origin.x, c.origin.y, c.origin.z],
        rotation: rotation_of_heading(heading),
    };
    p.heading_motion = c.heading_motion;
    let mut m = p.next_position(time, ON_GROUND);
    let new_cell = c.cell != before.cell;
    if c.flags & position_flags::JUMP != 0 && !new_cell {
        m.flags |= position_flags::JUMP;
        m.velocity = c.jump_velocity;
    }
    let jumped = c.flags & position_flags::JUMP != 0;
    presence::tell_position(s, net, world, &m, s.cfg.echo_positions, new_cell);
    let moved_view = c.cell >> 16 != before.cell >> 16
        || (presence::indoors(c.cell) && c.cell != before.cell)
        || presence::land_cell(c.cell, [c.origin.x, c.origin.y, c.origin.z])
            != presence::land_cell(before.cell, before.origin);
    if moved_view {
        presence::view_moved(s, net, world, clock);
    }
    if c.cell >> 16 != before.cell >> 16 {
        crate::kingdoms::check(s, net, world);
    }
    crate::places::arrived(s, net, world, clock);
    crate::places::walk_interrupted(s, net, world, c.flags, c.move_to_stamp);
    crate::gives::moved(s, net, world, clock);
    crate::places::walked(s, net, world, clock);
    crate::usable::chests::moved(s, net, world, clock);
    crate::inventory::moved(s, net, world);
    crate::skill_use::moved(s, net, world, clock);
    if jumped {
        // An item's use under way is cut short by a jump.
        crate::item_use::cut_short(s, net, world);
    }
    crate::social::moved(s, net, world, clock);
    crate::mounts::moved(s, world, clock);
    crate::workshops::moved(s, net, world, clock);
}

/// Keep an option the client sends. Returns whether the call was one the server keeps.
fn kept_call(p: &mut Player, function: u32, args: &[u8]) -> bool {
    if args.len() > crate::options::MOST_PART_BYTES {
        return false;
    }
    let Ok(v) = uploads::values(args) else {
        return false;
    };
    let table = |v: &Value| match v {
        Value::Native(uploads::kind::WORD_WORD_HASH, b) => Pairs::from_packed(b).ok(),
        _ => None,
    };
    match (function, v.as_slice()) {
        (
            request::GAME_OPTIONS,
            [Value::Int(bits), Value::Int(radar), Value::Int(height), Value::Float(range)],
        ) => {
            p.options
                .set_game_options(f32::from_bits(*range), *height, *radar, *bits);
            true
        }
        (request::SHORTCUT, [Value::Native(uploads::kind::SHORTCUT, s), Value::Int(place)]) => {
            p.options.set_shortcut(*place as usize, s.clone());
            true
        }
        (
            request::WINDOW_SETTINGS,
            [opacities, Value::Native(uploads::kind::WINDOW_PLACES, places)],
        ) => {
            let Some(o) = table(opacities) else {
                return false;
            };
            p.options.window_places = Some(places.clone());
            p.options.opacities = Some(o);
            true
        }
        (request::CHAT_OPTIONS, [popups, devoted, colors, sizes, filters]) => {
            let (Some(a), Some(b), Some(c), Some(d), Some(e)) = (
                table(popups),
                table(devoted),
                table(colors),
                table(sizes),
                table(filters),
            ) else {
                return false;
            };
            p.options.popup_flags = Some(a);
            p.options.devoted_windows = Some(b);
            p.options.font_colors = Some(c);
            p.options.font_sizes = Some(d);
            p.options.chat_filters = Some(e);
            true
        }
        _ => false,
    }
}
