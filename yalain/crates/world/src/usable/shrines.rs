//! Faction shrines: a player who uses one is walked to within 3 m of it (the order sent even when
//! it stands there), then the shrine and the player play their behaviors and the player is told
//! "Your Kingdom Rating has been recorded."; the use is never answered (2017: 9 of 9 uses). A
//! player on a mount is told then that the shrine's use is canceled (3 of 3 mounted arrivals; none
//! of 6 on foot).
//!
//! A player on foot of another kingdom than the shrine's (a player in none counts as of the
//! neutral kingdom) joins the shrine's kingdom 3.83 s later ([`crate::kingdoms::join`]; 2017: 1
//! of 1); one of its kingdom already is told nothing more (5 of 5).

use auberean_protocol::physics::ObjectEvent;
use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::places::{self, Use};
use crate::{kingdoms, progress, Clock, Server};

/// The script library's kind of the faction shrines.
pub const FACTION_SHRINES: [&str; 1] = ["FactionShrineTemplate"];

/// How near the shrine the player is walked, in meters.
pub const REACH: f32 = 3.0;

/// What the shrine and its user play.
pub const SHRINE_BEHAVIOR: u32 = 0x4100_0086;
pub const USER_BEHAVIOR: u32 = 0x4100_00F9;

/// "Your Kingdom Rating has been recorded.", and the kind of text it is shown as.
pub const RECORDED: (u32, u32) = (0x9DFE_C6A0, 0x2500_0014);
/// "Shrine use canceled."
pub const CANCELED: (u32, u32) = (0xB1DA_8AB3, 0x2500_0014);
pub const SHOWN_AS: u32 = 8;

/// A player asks to use a faction shrine: walked to it.
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: Use, clock: Clock) {
    places::walk_to(s, net, world, u, REACH);
    places::arrived(s, net, world, clock);
}

/// A player has come to a faction shrine (its walk stopped): the behaviors and the text (and the
/// use canceled for a mounted player, the shrine's kingdom joined by one on foot of another), and
/// no answer. `package` is the shrine's script package.
pub fn at_shrine<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    package: u32,
    clock: Clock,
) {
    let shrine = ServerMessage::PlayBehavior(ObjectEvent {
        sender: places::object_stamp(s, u.item),
        action: BehaviorParams::behavior(SHRINE_BEHAVIOR),
    });
    places::to_knowers(s, net, u.item, &shrine);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let plays = ServerMessage::PlayBehavior(places::player_event(
        p,
        BehaviorParams::behavior(USER_BEHAVIOR),
    ));
    let me = p.character;
    let mounted = crate::mounts::mounted(p);
    places::to_all_knowing(s, net, me, &plays);
    progress::private(
        net,
        world,
        &crate::chat::show_text(SHOWN_AS, progress::text(RECORDED)),
    );
    if mounted {
        progress::private(
            net,
            world,
            &crate::chat::show_text(SHOWN_AS, progress::text(CANCELED)),
        );
        return;
    }
    let Some(&shrine) = s.shrine_kingdoms.get(&package) else {
        return;
    };
    let mine = s
        .players
        .get(&world)
        .and_then(|p| kingdoms::of(&p.qualities));
    if !kingdoms::of_shrines_kingdom(mine, shrine) {
        kingdoms::join_after(s, world, shrine, clock);
    }
}
