//! The player's pets and its mount as Horizon draws them: the pets from the view, each named as
//! the game shows it, the pets the scripts said were attacked this frame, whether the player's
//! options show the pets, and whether the player rides a mount.

use auberean_client_sdk::contract::notice::{Notice, PetNotice, Said};
use auberean_client_sdk::contract::pet;
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::View;

/// The bit of the player's options that hides the pets.
pub const HIDE_PETS: u32 = 0x1_0000;

/// One of the player's pets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pet {
    pub id: u64,
    /// Its name, without its owner's.
    pub name: String,
    /// Its icon, a picture of the game's records (0 for none).
    pub icon: u32,
    /// The mode it has taken ([`pet::mode`]).
    pub mode: u32,
}

/// The player's pets and its mount.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pets {
    /// The pets, in the order they came.
    pub pets: Vec<Pet>,
    /// The pets the scripts said were attacked this frame.
    pub attacked: Vec<u64>,
    /// The player's options hide the pets.
    pub hidden: bool,
    /// The player rides a mount.
    pub mounted: bool,
}

impl Pets {
    /// Follow one frame: the view's pets, options and mount, and the scripts' notices of attacks.
    pub fn follow(
        &mut self,
        view: Option<&View>,
        notices: &[Notice],
        write: &dyn Fn(&TextRef) -> Option<String>,
    ) {
        self.attacked.clear();
        let Some(view) = view else {
            *self = Self::default();
            return;
        };
        self.pets = view
            .pets
            .iter()
            .map(|p| Pet {
                id: p.id,
                name: p
                    .name
                    .as_ref()
                    .and_then(write)
                    .map(|n| pet::shown_name(&n))
                    .unwrap_or_default(),
                icon: p.icon,
                mode: p.mode,
            })
            .collect();
        if let Some(o) = &view.options {
            self.hidden = o.bits & HIDE_PETS != 0;
        }
        self.mounted = view.player.as_ref().is_some_and(|p| p.mounted);
        for n in notices {
            if let Said::Pet(PetNotice::Attacked { pet }) = n.said {
                self.attacked.push(pet);
            }
        }
    }

    /// A pet by its id.
    #[must_use]
    pub fn pet(&self, id: u64) -> Option<&Pet> {
        self.pets.iter().find(|p| p.id == id)
    }

    /// Whether the pets are shown: the player has some and its options show them.
    #[must_use]
    pub fn shown(&self) -> bool {
        !self.pets.is_empty() && !self.hidden
    }
}
