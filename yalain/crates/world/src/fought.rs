//! Credit for a kill shared by those who fought it ([`yalain_content::fought`]): a world's step or
//! give may wait on its player having fought a camp's creature.
//!
//! **Fought.** As a camp's creature dies, the characters that harmed it (a pet's harm counted as
//! its owner's) are kept as those who fought the camp's last creature, in place of any kept before;
//! a creature's coming back changes nothing. A step or a give naming the camp runs for a player who
//! is among them, or who is a fellow of one now: a healer or a helper of the fight is most often
//! the fighters' fellow. Nothing of this is kept across a restart.

use yalain_net::ConnId;
use yalain_store::Store;

use crate::Server;

/// A camp's creature has died: those who harmed it, by the harm each took (a pet's counted as its
/// owner's), are kept as the ones who fought its last creature.
pub fn record<S: Store>(s: &mut Server<S>, camp: usize, parts: &[(u64, i64)]) {
    let mut fought: Vec<u64> = Vec::new();
    for (by, _) in parts {
        let owner = s.pets.by_id.get(by).map_or(*by, |p| p.owner);
        if !fought.contains(&owner) {
            fought.push(owner);
        }
    }
    if let Some(state) = s.camps.get_mut(camp) {
        state.fought = fought;
    }
}

/// Did a player, or a fellow of its now, harm each of these camps' last creature to die?
#[must_use]
pub fn fought_all<S: Store>(s: &Server<S>, world: ConnId, camps: &[u32]) -> bool {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return false;
    };
    let fellows: Vec<u64> = s
        .fellowships
        .of(me)
        .and_then(|i| s.fellowships.list.get(i))
        .map_or_else(|| vec![me], |f| f.fellows.iter().map(|x| x.id).collect());
    camps.iter().all(|c| {
        usize::try_from(*c)
            .ok()
            .and_then(|c| s.camps.get(c))
            .is_some_and(|state| {
                state
                    .fought
                    .iter()
                    .any(|by| *by == me || fellows.contains(by))
            })
    })
}

/// May a step run for a player, by the camps' creatures it waits on the player having fought?
#[must_use]
pub fn allows_step<S: Store>(
    s: &Server<S>,
    world: ConnId,
    step: &yalain_content::quests::Step,
) -> bool {
    let gates = &s.content.fought_gates;
    gates.is_empty()
        || gates
            .of_step(step)
            .is_none_or(|camps| fought_all(s, world, camps))
}

/// May a give take its item from a player, by the camps' creatures it waits on the player having
/// fought?
#[must_use]
pub fn allows_give<S: Store>(
    s: &Server<S>,
    world: ConnId,
    give: &yalain_content::quests::Give,
) -> bool {
    let gates = &s.content.fought_gates;
    gates.is_empty()
        || gates
            .of_give(give)
            .is_none_or(|camps| fought_all(s, world, camps))
}
