//! The classic interface's mouse pointer: which image it shows for the targeting mode, the
//! combat mode and the object under it, and the busy pointer that stays up while requests are
//! outstanding. Each image carries its own hotspot; native rendering owns visibility and clipping.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorArt {
    pub did: u32,
    pub width: u32,
    pub height: u32,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
}
impl CursorArt {
    const fn new(did: u32, width: u32, height: u32, x: i32, y: i32) -> Self {
        Self {
            did,
            width,
            height,
            hotspot_x: x,
            hotspot_y: y,
        }
    }
    pub fn origin(self, pointer_x: i32, pointer_y: i32) -> (i32, i32) {
        (pointer_x - self.hotspot_x, pointer_y - self.hotspot_y)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CursorInput {
    /// Raw classic mode, not the public target-mode enum. 4=use-with, 5=spell.
    pub mode: u32,
    pub combat_mode: u32,
    /// The HUD's alternate pointer state, not an inferred generic busy flag.
    pub alternate: bool,
    /// The current world/item hover path reports a target.
    pub hovered: bool,
    /// Result of the use-with or spell targeting predicate for that target.
    /// Unknown validity must not be treated as success.
    pub target_valid: bool,
}

/// None means the raw mode has no cursor assignment (retain the current one).
/// Drag and widget-specific cursors override this result in the native host.
pub fn resolve(input: CursorInput) -> Option<CursorArt> {
    let art = match input.mode {
        0 => {
            let (plain, hover, width) = if input.alternate {
                (0x06001930, 0x06001931, 20)
            } else {
                match input.combat_mode {
                    2 | 4 => (0x06001940, 0x06001941, 15),
                    8 => (0x06001958, 0x06001957, 15),
                    _ => (0x06000086, 0x0600123e, 15),
                }
            };
            CursorArt::new(if input.hovered { hover } else { plain }, width, 28, 0, 0)
        }
        1 | 7 | 8 | 9 => CursorArt::new(0x0600120c, 26, 28, 0, 0),
        2 => CursorArt::new(0x0600120b, 26, 32, 0, 0),
        3 => CursorArt::new(0x060010ec, 20, 28, 0, 0),
        4 => {
            if !input.hovered {
                CursorArt::new(0x060018e2, 28, 28, 14, 14)
            } else {
                CursorArt::new(
                    if input.target_valid {
                        0x060018e4
                    } else {
                        0x060018e3
                    },
                    32,
                    32,
                    14,
                    14,
                )
            }
        }
        5 => CursorArt::new(
            if !input.hovered {
                0x0600139c
            } else if input.target_valid {
                0x0600139b
            } else {
                0x0600139a
            },
            32,
            32,
            14,
            14,
        ),
        6 => CursorArt::new(0x0600144f, 32, 32, 0, 0),
        _ => return None,
    };
    Some(art)
}

/// Whether the object under the pointer is a valid target for the armed spell, without any
/// feedback line; spell mode 5 uses it for its pointer. The caller obtains
/// the mask from the armed spell/formula; unavailable spell metadata is not
/// equivalent to a known zero (untargeted) mask.
pub fn spell_target_valid(
    world: &dereth_client_model::World,
    object: Option<dereth_primitives::ObjectId>,
    target_mask: u32,
) -> bool {
    let object = object.filter(|id| id.0 != 0);
    if target_mask == 0 {
        return object.is_none();
    }
    let Some(id) = object else {
        return false;
    };
    if target_mask & 0x8107 == 0 && world.player == Some(id) {
        return false;
    }
    let Some(w) = world.weenie(id) else {
        return false;
    };
    if w.pwd.stack_size.unwrap_or(0) > 1 {
        return false;
    }
    if target_mask & w.inq_type() == 0 && target_mask & 0x8107 == 0 {
        return false;
    }
    if !w.is_player() && w.pwd.bitfield & 0x10 == 0 {
        return false;
    }
    world.player.is_some_and(|id| world.weenie(id).is_some())
}

/// The pointer shown before the game interface sets any other.
pub const fn pregame() -> CursorArt {
    CursorArt::new(0x06000086, 15, 28, 0, 0)
}

/// Counts outstanding begin/end operations; the busy pointer shows while any is open. The
/// host must route those semantic operations; a missing end must not be hidden by a timer.
#[derive(Clone, Copy, Debug, Default)]
pub struct BusyCounter {
    depth: i16,
}
impl BusyCounter {
    pub fn begin(&mut self) {
        self.depth = self.depth.wrapping_add(1);
    }
    pub fn end(&mut self) {
        self.depth = self.depth.max(1) - 1;
    }
    pub fn active(self) -> bool {
        self.depth != 0
    }
}

/// The dragged-item pointer: this decoration drawn over the object's icon, overlay and border.
/// It omits both the item-type background and the icon underlay used in a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragArt {
    pub width: u32,
    pub height: u32,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
    pub decoration: CursorArt,
}
pub const fn drag(raw_type: u16) -> DragArt {
    DragArt {
        width: 32,
        height: 32,
        hotspot_x: 0,
        hotspot_y: 0,
        decoration: if raw_type == 2 || raw_type == 3 {
            CursorArt::new(0x06001348, 32, 32, 0, 0)
        } else {
            CursorArt::new(0x060011c4, 8, 8, 0, 0)
        },
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn target_cursor_keeps_fourteen_pixel_hotspot_when_art_grows() {
        let mut i = CursorInput {
            mode: 4,
            ..Default::default()
        };
        let neutral = resolve(i).unwrap();
        assert_eq!(
            (neutral.did, neutral.width, neutral.origin(100, 200)),
            (0x060018e2, 28, (86, 186))
        );
        i.hovered = true;
        assert_eq!(resolve(i).unwrap().did, 0x060018e3);
        i.target_valid = true;
        let valid = resolve(i).unwrap();
        assert_eq!(
            (valid.did, valid.width, valid.origin(100, 200)),
            (0x060018e4, 32, (86, 186))
        );
        i.mode = 5;
        assert_eq!(resolve(i).unwrap().did, 0x0600139b);
    }
    #[test]
    fn alternate_cursor_precedes_combat_and_magic_modes() {
        let mut i = CursorInput {
            combat_mode: 8,
            hovered: true,
            ..Default::default()
        };
        assert_eq!(resolve(i).unwrap().did, 0x06001957);
        i.alternate = true;
        assert_eq!(resolve(i).unwrap().did, 0x06001931);
        i.mode = 2;
        assert_eq!(resolve(i).unwrap().did, 0x0600120b);
        i.mode = 100;
        assert!(resolve(i).is_none());
    }
    #[test]
    fn busy_depth_requires_all_ends_but_extra_end_does_not_underflow() {
        let mut c = BusyCounter::default();
        c.begin();
        c.begin();
        c.end();
        assert!(c.active());
        c.end();
        assert!(!c.active());
        c.end();
        assert!(!c.active());
        assert_eq!(pregame().origin(0, 0), (0, 0));
    }
    #[test]
    fn ordinary_drag_has_small_arrow_and_types_two_three_full_decoration() {
        assert_eq!(drag(1).decoration.did, 0x060011c4);
        assert_eq!(drag(1).decoration.width, 8);
        assert_eq!(drag(2), drag(3));
        assert_eq!(drag(2).decoration.did, 0x06001348);
        assert_eq!(drag(2).hotspot_x, 0);
    }
    #[test]
    fn classic_spell_cursor_uses_spell_mask_and_rejects_stacks() {
        use dereth_client_model::{weenie::Weenie, World};
        use dereth_primitives::ObjectId;
        let mut w = World::new();
        w.player = Some(ObjectId(1));
        w.tables
            .weenies
            .insert(ObjectId(1), Weenie::new(ObjectId(1)));
        let mut target = Weenie::new(ObjectId(2));
        target.pwd.obj_type = 0x10;
        target.pwd.bitfield = 0x10;
        target.pwd.pet_owner = Some(ObjectId(3));
        w.tables.weenies.insert(ObjectId(2), target);
        assert!(spell_target_valid(&w, Some(ObjectId(2)), 0x10));
        assert!(!spell_target_valid(&w, Some(ObjectId(2)), 0x2000));
        assert!(!spell_target_valid(&w, Some(ObjectId(1)), 0x10));
        assert!(spell_target_valid(&w, None, 0));
        assert!(!spell_target_valid(&w, Some(ObjectId(2)), 0));
        w.weenie_mut(ObjectId(2)).unwrap().pwd.stack_size = Some(2);
        assert!(!spell_target_valid(&w, Some(ObjectId(2)), 0x10));
    }
}
