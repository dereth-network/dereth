//! The player's pets and its mount, kept for every front end ([`Pets`]) from what the player's
//! scripts tell the panels.
//!
//! - A pet comes with its name, its icon and its mode, after those already here (one that comes
//!   again keeps its place, its name, icon and mode those it comes with); it goes; it is given a
//!   name; it takes a mode.
//! - The player mounts, or gets off.
//!
//! Every pet goes, and the player is on foot again, when the player leaves the world or logs off.
//! A pet under attack is a moment, not a state: front ends hear it in the notices.

use auberean_client_contract::notice::{Notice, PetNotice, PlayerNotice, Said};
use auberean_client_contract::view::Pet;

/// The player's pets, in the order they came, and whether the player rides a mount.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pets {
    list: Vec<Pet>,
    mounted: bool,
}

impl Pets {
    /// What a notice of the player's scripts says of the pets or the mount.
    // D2-02383, D2-02377, D2-02392, D2-02389, D2-02452
    pub fn hear(&mut self, n: &Notice) {
        match &n.said {
            Said::Pet(PetNotice::Added {
                pet,
                name,
                icon,
                mode,
            }) => {
                let came = Pet {
                    id: *pet,
                    name: name.clone(),
                    icon: *icon,
                    mode: *mode,
                };
                match self.list.iter_mut().find(|p| p.id == *pet) {
                    Some(p) => *p = came,
                    None => self.list.push(came),
                }
            }
            Said::Pet(PetNotice::Removed { pet }) => self.list.retain(|p| p.id != *pet),
            Said::Pet(PetNotice::Named { pet, name }) => {
                if let Some(p) = self.list.iter_mut().find(|p| p.id == *pet) {
                    p.name.clone_from(name);
                }
            }
            Said::Pet(PetNotice::Mode { pet, mode }) => {
                if let Some(p) = self.list.iter_mut().find(|p| p.id == *pet) {
                    p.mode = *mode;
                }
            }
            Said::Player(PlayerNotice::Mounted { mounted }) => self.mounted = *mounted,
            _ => {}
        }
    }

    /// The pets, in the order they came.
    #[must_use]
    pub fn pets(&self) -> &[Pet] {
        &self.list
    }

    /// Whether the player rides a mount.
    #[must_use]
    pub const fn mounted(&self) -> bool {
        self.mounted
    }

    /// No pets, and on foot: the player left the world.
    pub fn clear(&mut self) {
        self.list.clear();
        self.mounted = false;
    }
}
