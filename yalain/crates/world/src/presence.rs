//! Who sees what: the objects and players a client is told of, as they come into and leave its
//! view, and the movements of the ones it knows.
//!
//! **View.** A client is told of the objects within its view: outdoors, the land cells (20 m on a
//! side) up to three away from its player's in each direction; indoors, its player's own cell and
//! the cells that cell sees (each interior cell lists the cells visible from it), the others only
//! once the client has drawn its scene. An object is made for it each time it comes into view,
//! players both ways: a client never draws an object made in an interior cell it does not hold,
//! so nothing is made there before the player's cell sees it. The server keeps telling a client
//! of an object it was told of while the object stays in the player's landblock (indoors) or
//! within three land cells (outdoors). Nothing is sent as an object leaves that: the client lets
//! it go itself; the server only stops telling it of the object. An object leaving the world is
//! sent away: a player that leaves is seen to leave by every client that holds it.
//!
//! **Movement.** A player's place, its head turns and what it looks at reach every client whose
//! view holds it, each on that client's own stream for that object and kind (the place and the
//! leaving on one stream, the head on another), so a newer one supersedes an older one still in
//! flight. A place in a new cell goes as a cell position.

use auberean_protocol::physics::{HeadDirection, Leave, ObjectEvent, ObjectPosition, Target};
use auberean_protocol::types::ObjectStamp;
use auberean_protocol::{queue, ServerMessage};
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::player;
use crate::world::{Player, CONTENT_ID_BASE};
use crate::{Clock, Server};

/// What a client knows of one object: the streams the object's positions and head turns reach
/// it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub position_stream: u32,
    pub head_stream: u32,
}

pub use auberean_physics::cells::FIRST_INTERIOR_CELL;

/// Is a cell inside (an interior cell rather than one of the landblock's outdoor cells)?
#[must_use]
pub const fn indoors(cell: u32) -> bool {
    cell & 0xFFFF >= FIRST_INTERIOR_CELL
}

/// How far the view reaches outdoors, in land cells each way.
pub const VIEW_CELLS: i64 = 3;

/// A land cell's side, in meters.
pub const CELL_SIDE: f32 = auberean_physics::land::CELL_LENGTH;

/// The land cell a point of a landblock lies in, counted across the whole world (x, y).
#[must_use]
pub fn land_cell(cell: u32, origin: [f32; 3]) -> (i64, i64) {
    #[allow(clippy::cast_possible_truncation)]
    let at = |block: u32, v: f32| -> i64 {
        i64::from(block) * 8 + (v / CELL_SIDE).floor().clamp(-1.0, 8.0) as i64
    };
    (at(cell >> 24, origin[0]), at(cell >> 16 & 0xFF, origin[1]))
}

/// Is an object at a place near enough to a player at another for its client to be told of it:
/// in one landblock when either is indoors, within three land cells outdoors? An object the
/// client was told of is kept while this holds; [`in_sight`] decides when it is made.
#[must_use]
pub fn in_view(viewer: &StoredPosition, cell: u32, origin: [f32; 3]) -> bool {
    if indoors(viewer.cell) || indoors(cell) {
        return viewer.cell >> 16 == cell >> 16;
    }
    let (ax, ay) = land_cell(viewer.cell, viewer.origin);
    let (bx, by) = land_cell(cell, origin);
    (ax - bx).abs() <= VIEW_CELLS && (ay - by).abs() <= VIEW_CELLS
}

/// May an object at a place (a player, an object of the world, a thing the world has made) be
/// made for a player now? As [`in_view`], except that indoors it is made only once the player's
/// client holds its cell: the player's own cell, or (its scene drawn) a cell its own cell sees,
/// when the host knows what each cell sees (else any cell of the place, once the scene is drawn).
/// An object made in an interior cell the client has not loaded is never drawn by it (selectable,
/// labelled, unseen); one made once the client holds the cell is. 2017's servers made a player in
/// an interior the objects of its own cell and of the cells it sees, the others as it went.
#[must_use]
pub fn in_sight(
    ground: Option<&dyn crate::Ground>,
    viewer: &Player,
    cell: u32,
    origin: [f32; 3],
) -> bool {
    if !in_view(&viewer.position, cell, origin) {
        return false;
    }
    if !indoors(cell) || cell == viewer.position.cell {
        return true;
    }
    if !viewer.scene_drawn {
        return false;
    }
    match ground.and_then(|g| g.sees(viewer.position.cell)) {
        Some(seen) => seen.contains(&cell),
        None => true,
    }
}

/// The id the world gives a content object.
#[must_use]
pub fn content_id(s: &[yalain_content::ContentObject], i: usize) -> u64 {
    s[i].object.unwrap_or(CONTENT_ID_BASE | (i as u64 + 1))
}

fn send(net: &mut ServerNet, conn: ConnId, kind: BlobKind, m: &ServerMessage) {
    net.send(conn, queue::EVENT, kind, m.encode());
}

/// Tell a client of an object it does not know yet: make it, with streams for it.
fn learn(net: &mut ServerNet, viewer: &mut Player, world: ConnId, id: u64, m: &ServerMessage) {
    tracing::debug!(
        ?world,
        object = format_args!("{id:#x}"),
        viewer_cell = format_args!("{:08X}", viewer.position.cell),
        "made for a client"
    );
    send(net, world, BlobKind::UNORDERED, m);
    let seen = Seen {
        position_stream: net.new_stream(world).unwrap_or(0),
        head_stream: net.new_stream(world).unwrap_or(0),
    };
    viewer.known.insert(id, seen);
}

/// Make an object for another player's client that does not know it yet.
pub(crate) fn make_for<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    id: u64,
    m: &ServerMessage,
) {
    if let Some(mut viewer) = s.players.remove(&conn) {
        learn(net, &mut viewer, conn, id, m);
        s.players.insert(conn, viewer);
    }
}

/// The other players' clients that know a player, and whether each knows an object too.
pub(crate) fn watching<S: Store>(
    s: &Server<S>,
    world: ConnId,
    player: u64,
    object: u64,
) -> Vec<(ConnId, bool)> {
    s.players
        .iter()
        .filter(|(c, q)| **c != world && q.known.contains_key(&player))
        .map(|(c, q)| (*c, q.known.contains_key(&object)))
        .collect()
}

/// The weapons a creature holds (none for anything else).
fn weapons<S: Store>(s: &Server<S>, id: u64) -> Vec<u64> {
    s.things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.weapons.clone())
        .unwrap_or_default()
}

/// Stop telling a client of an object that left its view (nothing is sent).
fn lose(viewer: &mut Player, id: u64) {
    viewer.known.remove(&id);
}

/// What a player holds in its hands, as another client is told of it when the player comes into
/// its view: each item made hung from the hand (what is worn on the body is in the player's look).
pub fn held_seen<S: Store>(s: &Server<S>, p: &Player) -> Vec<(u64, ServerMessage)> {
    p.play
        .items
        .iter()
        .filter(|h| h.location != 0 && h.location & crate::equip::BODY_PLACES == 0)
        .filter_map(|h| {
            let t = s.things.get(h.id)?;
            Some((
                h.id,
                ServerMessage::CreateObject(crate::inventory::held_create(
                    &s.catalog, t, p.instance,
                )),
            ))
        })
        .collect()
}

/// The ids of what a player holds in its hands.
fn in_hand(p: &Player) -> Vec<u64> {
    p.play
        .items
        .iter()
        .filter(|h| h.location != 0 && h.location & crate::equip::BODY_PLACES == 0)
        .map(|h| h.id)
        .collect()
}

/// The player's object as another client is told of it: where it is now, with its stamps.
#[must_use]
pub fn player_seen(p: &Player) -> ServerMessage {
    let mut o = player::player_object(
        p.character,
        &p.shown_name(),
        p.sex,
        crate::mounts::shown_look(p).clone(),
        player::placement(&p.position),
        p.instance,
        &player::Marks::of(&crate::quals::read(&p.qualities)),
    );
    o.physics.visual_ordering_stamp = p.visual;
    o.physics.stamps = p.physics_stamps();
    // A rider is seen at its mount's speed.
    if let Some(v) = crate::mounts::shown_speed(p) {
        o.physics.bits |= auberean_protocol::types::PhysicsDesc::VELOCITY_SCALE;
        o.physics.velocity_scale = v;
    }
    ServerMessage::CreateObject(o)
}

/// How often a player's own reports bring its view up to date, in seconds. Bringing it up to date
/// walks all the world has placed, so what a client reports doesn't set how often that's done. 2017's
/// clients reported at most 35 places in a second, 11 or fewer in 99.9% of seconds, and crossed
/// fewer lines than that: what's owed comes a moment later, at the world's next pass past this.
pub const VIEW_EVERY: f64 = 0.25;

/// A player's report took it to where its view changes: its view is brought up to date now, or as
/// soon as [`VIEW_EVERY`] has passed.
pub fn view_moved<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if clock.local.0 - p.view_brought < VIEW_EVERY {
        p.view_owed = true;
        return;
    }
    p.view_brought = clock.local.0;
    p.view_owed = false;
    refresh(s, net, world);
}

/// The views owed since their players last reported, brought up to date as they come due.
pub fn views_owed<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let due: Vec<ConnId> = s
        .players
        .iter()
        .filter(|(_, p)| p.view_owed && clock.local.0 - p.view_brought >= VIEW_EVERY)
        .map(|(world, _)| *world)
        .collect();
    for world in due {
        s.saves.piece_begins();
        view_moved(s, net, world, clock);
    }
}

/// Bring a player's view up to date: make what has come into it and stop telling it of what has
/// left it, and do the same for every other player as regards this one.
pub fn refresh<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(mut p) = s.players.remove(&world) else {
        return;
    };
    let here = p.position;
    let ground = s.ground.as_deref();
    // The content objects.
    for i in 0..s.content.objects.len() {
        let id = content_id(&s.content.objects, i);
        let o = &s.content.objects[i];
        let known = p.known.contains_key(&id);
        if !known && in_sight(ground, &p, o.cell, o.origin) {
            let m = ServerMessage::CreateObject(crate::usable::doors::created(s, id, o));
            learn(net, &mut p, world, id, &m);
        } else if known && !in_view(&here, o.cell, o.origin) {
            lose(&mut p, id);
        }
    }
    // The things the world has made.
    let ids: Vec<u64> = s.things.placed().map(|t| t.id).collect();
    for id in ids {
        let Some(t) = s.things.get(id) else { continue };
        let Some(at) = t.at else { continue };
        let known = p.known.contains_key(&id);
        if !known && in_sight(ground, &p, at.cell, at.origin) {
            let m = ServerMessage::CreateObject(crate::things::create(t));
            let block = crate::death::corpse_block(t);
            learn(net, &mut p, world, id, &m);
            // A corpse is followed by the changes it was made up of.
            for m in &block {
                send(net, world, BlobKind::VISUAL, m);
            }
            // A creature fighting is made with its weapons drawn.
            for (w, m) in crate::arms::seen_with(s, id) {
                learn(net, &mut p, world, w, &m);
            }
        } else if known && !in_view(&here, at.cell, at.origin) {
            lose(&mut p, id);
            for w in weapons(s, id) {
                lose(&mut p, w);
            }
        }
    }
    // The other players, both ways: each made for the other once its own view holds it.
    let others: Vec<ConnId> = s.players.keys().copied().collect();
    for o in others {
        let Some(mut q) = s.players.remove(&o) else {
            continue;
        };
        if in_view(&here, q.position.cell, q.position.origin) {
            let (qc, qo) = (q.position.cell, q.position.origin);
            // Each made with what it holds in its hands.
            if !p.known.contains_key(&q.character) && in_sight(ground, &p, qc, qo) && sees(&p, &q) {
                learn(net, &mut p, world, q.character, &player_seen(&q));
                for (id, m) in held_seen(s, &q) {
                    learn(net, &mut p, world, id, &m);
                }
            }
            if !q.known.contains_key(&p.character)
                && in_sight(ground, &q, here.cell, here.origin)
                && sees(&q, &p)
            {
                learn(net, &mut q, o, p.character, &player_seen(&p));
                for (id, m) in held_seen(s, &p) {
                    learn(net, &mut q, o, id, &m);
                }
            }
        } else {
            lose(&mut p, q.character);
            lose(&mut q, p.character);
            for id in in_hand(&q) {
                lose(&mut p, id);
            }
            for id in in_hand(&p) {
                lose(&mut q, id);
            }
        }
        s.players.insert(o, q);
    }
    s.players.insert(world, p);
}

/// May a client be told of a player? Not of a cloaked admin, unless it sees what is hidden.
fn sees(viewer: &Player, other: &Player) -> bool {
    !other.admin.cloaked || viewer.admin.sees_hidden
}

/// The player a client names by its id, when that client may be told of it: to a client that may
/// not, a cloaked admin is as much not there as an id nobody has. A client knows its own player.
/// Every id a client sends about a player goes through this, not [`crate::social::world_of`].
#[must_use]
pub fn player_for<S: Store>(s: &Server<S>, viewer: ConnId, id: u64) -> Option<ConnId> {
    let me = s.players.get(&viewer)?;
    s.players
        .iter()
        .find(|(w, q)| q.character == id && (**w == viewer || sees(me, q)))
        .map(|(w, _)| *w)
}

/// The player a client names by its lowercase name, as [`player_for`] does by its id.
#[must_use]
pub fn player_named_for<S: Store>(s: &Server<S>, viewer: ConnId, name: &str) -> Option<ConnId> {
    let me = s.players.get(&viewer)?;
    player_named(s, name, |w, q| w == viewer || sees(me, q))
}

/// The player of a lowercase name among those `may` lets through. A name with spaces may be one
/// player's name or another's with its suffix, the name as a client shows it: its own name wins.
pub(crate) fn player_named<S: Store>(
    s: &Server<S>,
    name: &str,
    may: impl Fn(ConnId, &Player) -> bool,
) -> Option<ConnId> {
    let among = |answers: &dyn Fn(&Player) -> bool| {
        s.players
            .iter()
            .find(|(w, q)| answers(q) && may(**w, q))
            .map(|(w, _)| *w)
    };
    among(&|q| q.name.to_lowercase() == name)
        .or_else(|| among(&|q| !q.play.suffix.is_empty() && q.shown_name().to_lowercase() == name))
}

/// The characters in the world that are cloaked admins: the friends service and the network are not
/// to know they are here.
#[must_use]
pub fn cloaked_characters<S: Store>(s: &Server<S>) -> std::collections::HashSet<u64> {
    s.players
        .values()
        .filter(|p| p.admin.cloaked)
        .map(|p| p.character)
        .collect()
}

/// The characters in the world with a suffix, as a client shows each and by its own name, for the
/// friends service, which is told of no cloaked admin.
#[must_use]
pub fn suffixed_characters<S: Store>(s: &Server<S>) -> Vec<(String, String)> {
    s.players
        .values()
        .filter(|p| !p.play.suffix.is_empty() && !p.admin.cloaked)
        .map(|p| (p.shown_name(), p.name.clone()))
        .collect()
}

/// An admin was cloaked, or no longer is: the clients that may no longer see it see it leave, and
/// those that may now are told of it.
pub fn cloak_changed<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.remove(&world) else {
        return;
    };
    if p.admin.cloaked {
        leave_from(s, net, &p, |q| !q.admin.sees_hidden);
    }
    s.players.insert(world, p);
    refresh(s, net, world);
}

/// A player no longer sees what is hidden: every cloaked admin its client holds leaves it.
pub fn stops_seeing_hidden<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(viewer) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let cloaked: Vec<ConnId> = s
        .players
        .iter()
        .filter(|(w, q)| **w != world && q.admin.cloaked)
        .map(|(w, _)| *w)
        .collect();
    for c in cloaked {
        if let Some(q) = s.players.remove(&c) {
            leave_from(s, net, &q, |v| v.character == viewer);
            s.players.insert(c, q);
        }
    }
}

/// A player leaves the world: every client that holds it sees it leave, on its stream of the
/// player's places, with the player's next position stamp.
pub fn depart<S: Store>(s: &mut Server<S>, net: &mut ServerNet, p: &Player) {
    leave_from(s, net, p, |_| true);
}

/// The clients holding a player that `which` picks see it leave the world, and are no longer
/// told of it.
pub fn leave_from<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    p: &Player,
    which: impl Fn(&Player) -> bool,
) {
    let m = ServerMessage::ObjectLeaves(ObjectEvent {
        sender: p.stamp(),
        action: Leave {
            stamp: p.position_stamp.wrapping_add(1),
        },
    })
    .encode();
    let held = in_hand(p);
    for (conn, q) in &mut s.players {
        if !which(q) {
            continue;
        }
        if let Some(seen) = q.known.remove(&p.character) {
            net.send_ephemeral(*conn, queue::EVENT, seen.position_stream, m.clone());
            // What it held goes with it.
            for id in &held {
                q.known.remove(id);
            }
        }
    }
}

/// Send an event about a player to every other client that knows it, on that client's stream of
/// the kind `head` picks (head turns) or its position stream.
fn to_observers<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    from: ConnId,
    id: u64,
    head: bool,
    m: &ServerMessage,
) {
    let bytes = m.encode();
    for (conn, q) in &s.players {
        if *conn == from {
            continue;
        }
        if let Some(seen) = q.known.get(&id) {
            let stream = if head {
                seen.head_stream
            } else {
                seen.position_stream
            };
            net.send_ephemeral(*conn, queue::EVENT, stream, bytes.clone());
        }
    }
}

/// A player's place, to itself (when the server echoes) and to every client that knows it: a
/// place in a new cell as a cell position.
pub fn tell_position<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    m: &ObjectPosition,
    echo: bool,
    new_cell: bool,
) {
    let msg = if new_cell {
        ServerMessage::PositionCell(*m)
    } else {
        ServerMessage::Position(*m)
    };
    if echo {
        if let Some(p) = s.players.get(&world) {
            net.send_ephemeral(world, queue::EVENT, p.position_stream, msg.encode());
        }
    }
    to_observers(s, net, world, m.sender.id, false, &msg);
}

/// A player turned its head: to itself (when the server echoes) and to every client that knows
/// it.
pub fn tell_head<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    h: HeadDirection,
    echo: bool,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let m = ServerMessage::TurnHead(ObjectEvent {
        sender: p.stamp(),
        action: h,
    });
    if echo {
        net.send_ephemeral(world, queue::EVENT, p.head_stream, m.encode());
    }
    let id = p.character;
    to_observers(s, net, world, id, true, &m);
}

/// A player looks at an object: every client that knows it is told.
pub fn tell_look_at<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, target: u64) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.visual = p.visual.wrapping_add(1);
    let m = ServerMessage::LookAt(ObjectEvent {
        sender: ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
        action: Target(target),
    });
    let bytes = m.encode();
    let id = p.character;
    send(net, world, BlobKind::VISUAL, &m);
    for (conn, q) in &s.players {
        if *conn != world && q.known.contains_key(&id) {
            net.send(*conn, queue::EVENT, BlobKind::VISUAL, bytes.clone());
        }
    }
}

/// The client asks for an object again: make it again if it is in sight (a thing, a content
/// object or a player), else say nothing.
pub fn force_create<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, id: u64) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let ground = s.ground.as_deref();
    let m = if id == p.character {
        Some(player_seen(p))
    } else if let Some(t) = s.things.get(id) {
        t.at.filter(|at| in_sight(ground, p, at.cell, at.origin))
            .map(|_| ServerMessage::CreateObject(crate::things::create(t)))
    } else if let Some(q) = s.players.values().find(|q| q.character == id) {
        (in_sight(ground, p, q.position.cell, q.position.origin) && sees(p, q))
            .then(|| player_seen(q))
    } else {
        content_by_id(&s.content.objects, id).and_then(|i| {
            let o = &s.content.objects[i];
            in_sight(ground, p, o.cell, o.origin)
                .then(|| ServerMessage::CreateObject(crate::usable::doors::created(s, id, o)))
        })
    };
    let Some(m) = m else {
        s.unhandled.note("asked again for an object not in view");
        return;
    };
    send(net, world, BlobKind::UNORDERED, &m);
    if let Some(p) = s.players.get_mut(&world) {
        if id != p.character && !p.known.contains_key(&id) {
            let seen = Seen {
                position_stream: net.new_stream(world).unwrap_or(0),
                head_stream: net.new_stream(world).unwrap_or(0),
            };
            p.known.insert(id, seen);
        }
    }
}

/// The place in the pack of the content object with this id.
#[must_use]
pub fn content_by_id(objects: &[yalain_content::ContentObject], id: u64) -> Option<usize> {
    (0..objects.len()).find(|&i| content_id(objects, i) == id)
}

/// The distance between two places in one landblock or neighboring ones (else none):
/// landblocks are 160 m on a side.
#[must_use]
pub fn distance(a: &StoredPosition, b: &StoredPosition) -> Option<f32> {
    let (la, lb) = (a.cell >> 16, b.cell >> 16);
    let dx = i64::from(la >> 8) - i64::from(lb >> 8);
    let dy = i64::from(la & 0xFF) - i64::from(lb & 0xFF);
    if dx.abs() > 1 || dy.abs() > 1 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (ox, oy) = (dx as f32 * 160.0, dy as f32 * 160.0);
    let x = a.origin[0] + ox - b.origin[0];
    let y = a.origin[1] + oy - b.origin[1];
    let z = a.origin[2] - b.origin[2];
    Some((x * x + y * y + z * z).sqrt())
}

/// A thing has appeared in the world: every client whose view holds it is told of it (with its
/// entry effect, if it has one).
pub fn thing_appears<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(t) = s.things.get(id) else { return };
    let Some(at) = t.at else { return };
    let m = ServerMessage::CreateObject(crate::things::create(t));
    let viewers: Vec<ConnId> = s
        .players
        .iter()
        .filter(|(_, p)| {
            in_sight(s.ground.as_deref(), p, at.cell, at.origin) && !p.known.contains_key(&id)
        })
        .map(|(c, _)| *c)
        .collect();
    let drawn = crate::arms::seen_with(s, id);
    for c in viewers {
        if let Some(mut p) = s.players.remove(&c) {
            learn(net, &mut p, c, id, &m);
            for (w, m) in &drawn {
                learn(net, &mut p, c, *w, m);
            }
            s.players.insert(c, p);
        }
    }
    if let Some(t) = s.things.get_mut(id) {
        t.fresh = false;
    }
}

/// A thing has left the world: every client that knows it is told to forget it.
pub fn thing_gone<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, instance: u16) {
    let m = ServerMessage::ForgetObject(auberean_protocol::physics::ObjectOnly(ObjectStamp {
        id,
        instance_stamp: instance,
        other_stamp: 0,
    }));
    let bytes = m.encode();
    for (conn, p) in &mut s.players {
        if p.known.remove(&id).is_some() {
            net.send(*conn, queue::EVENT, BlobKind::UNORDERED, bytes.clone());
        }
    }
}

/// Send an event about a thing to every client that knows it, as a blob of a kind.
pub fn to_thing_knowers<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    id: u64,
    kind: BlobKind,
    m: &ServerMessage,
) {
    let bytes = m.encode();
    for (conn, p) in &s.players {
        if p.known.contains_key(&id) {
            net.send(*conn, queue::EVENT, kind, bytes.clone());
        }
    }
}

/// Send a message about an object to a client that knows it, on its stream of that object.
pub(crate) fn on_its_stream<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    id: u64,
    bytes: Vec<u8>,
) {
    if let Some(seen) = s.players.get(&conn).and_then(|p| p.known.get(&id)) {
        net.send_ephemeral(conn, queue::EVENT, seen.position_stream, bytes);
    }
}

/// An object leaves: every client that knows it is told, on its stream of that object.
pub(crate) fn leaves<S: Store>(s: &Server<S>, net: &mut ServerNet, id: u64, m: &ServerMessage) {
    let bytes = m.encode();
    for (conn, p) in &s.players {
        if let Some(seen) = p.known.get(&id) {
            net.send_ephemeral(*conn, queue::EVENT, seen.position_stream, bytes.clone());
        }
    }
}

/// A content object (a person walking its route) moved: told to every client that knows it and
/// still sees it, made for one that now sees it, let go by one that no longer does.
pub fn content_moves<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    index: usize,
    m: &ObjectPosition,
    new_cell: bool,
) {
    let Some(o) = s.content.objects.get(index) else {
        return;
    };
    let id = content_id(&s.content.objects, index);
    let (cell, origin) = (o.cell, o.origin);
    let create = ServerMessage::CreateObject(crate::usable::doors::created(s, id, o));
    let msg = if new_cell {
        ServerMessage::PositionCell(*m)
    } else {
        ServerMessage::Position(*m)
    }
    .encode();
    let conns: Vec<ConnId> = s.players.keys().copied().collect();
    for c in conns {
        let Some(mut p) = s.players.remove(&c) else {
            continue;
        };
        let known = p.known.get(&id).copied();
        let visible = if known.is_some() {
            in_view(&p.position, cell, origin)
        } else {
            in_sight(s.ground.as_deref(), &p, cell, origin)
        };
        match (visible, known) {
            (true, Some(seen)) => {
                net.send_ephemeral(c, queue::EVENT, seen.position_stream, msg.clone());
            }
            (true, None) => learn(net, &mut p, c, id, &create),
            (false, Some(_)) => lose(&mut p, id),
            (false, None) => {}
        }
        s.players.insert(c, p);
    }
}

/// A content object placed anew (the world database reloaded): made for every client whose view
/// holds it. One placed again (`was`, the instance stamp its clients hold) is sent anew, with its
/// newer instance stamp, to every client that holds it and still sees it, which replaces the one
/// it holds; a client that holds it and no longer sees it forgets it.
pub(crate) fn content_placed<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    index: usize,
    was: Option<u16>,
) {
    let Some(o) = s.content.objects.get(index) else {
        return;
    };
    let id = content_id(&s.content.objects, index);
    let (cell, origin) = (o.cell, o.origin);
    let create = ServerMessage::CreateObject(crate::usable::doors::created(s, id, o));
    let forget = was.map(|instance_stamp| {
        ServerMessage::ForgetObject(auberean_protocol::physics::ObjectOnly(ObjectStamp {
            id,
            instance_stamp,
            other_stamp: 0,
        }))
    });
    let conns: Vec<ConnId> = s.players.keys().copied().collect();
    for c in conns {
        let Some(mut p) = s.players.remove(&c) else {
            continue;
        };
        if p.known.contains_key(&id) {
            if in_view(&p.position, cell, origin) {
                send(net, c, BlobKind::UNORDERED, &create);
            } else {
                if let Some(m) = &forget {
                    send(net, c, BlobKind::UNORDERED, m);
                }
                lose(&mut p, id);
            }
        } else if in_sight(s.ground.as_deref(), &p, cell, origin) {
            learn(net, &mut p, c, id, &create);
        }
        s.players.insert(c, p);
    }
}

/// A thing has moved: every client that knows it and still sees it gets its place on its stream
/// of the thing's places; a client whose view it has entered is told of it; one whose view it has
/// left stops being told of it. A pet come into another cell first takes the movement ethereality
/// of where it now stands ([`crate::pets::settle_ethereality`]).
pub fn thing_moves<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    m: &ObjectPosition,
    new_cell: bool,
) {
    if new_cell && s.pets.is_pet(id) {
        crate::pets::settle_ethereality(s, net, id);
    }
    let Some(t) = s.things.get(id) else { return };
    let Some(at) = t.at else { return };
    let create = ServerMessage::CreateObject(crate::things::create(t));
    let drawn = crate::arms::seen_with(s, id);
    let held = weapons(s, id);
    let msg = if new_cell {
        ServerMessage::PositionCell(*m)
    } else {
        ServerMessage::Position(*m)
    }
    .encode();
    let conns: Vec<ConnId> = s.players.keys().copied().collect();
    for c in conns {
        let Some(mut p) = s.players.remove(&c) else {
            continue;
        };
        let known = p.known.get(&id).copied();
        let visible = if known.is_some() {
            in_view(&p.position, at.cell, at.origin)
        } else {
            in_sight(s.ground.as_deref(), &p, at.cell, at.origin)
        };
        match (visible, known) {
            (true, Some(seen)) => {
                net.send_ephemeral(c, queue::EVENT, seen.position_stream, msg.clone());
            }
            (true, None) => {
                learn(net, &mut p, c, id, &create);
                for (w, m) in &drawn {
                    learn(net, &mut p, c, *w, m);
                }
            }
            (false, Some(_)) => {
                lose(&mut p, id);
                for w in &held {
                    lose(&mut p, *w);
                }
            }
            (false, None) => {}
        }
        s.players.insert(c, p);
    }
}
