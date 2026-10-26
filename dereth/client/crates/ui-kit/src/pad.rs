//! What a pad hands the kit beside the focus ([`crate::nav`]): which half of an interface's
//! cross hotbars a trigger holds up, and the on-screen keyboard a line of text brings up.

use crate::nav::Dir;

/// Which half of the cross hotbars a trigger held brings up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossSet {
    /// LT: the left half.
    Left,
    /// RT: the right half.
    Right,
}

/// A key of the on-screen keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OskKey {
    Char(char),
    Shift,
    Space,
    Back,
    Enter,
    Done,
}

/// The on-screen keyboard's rows of letters; a row of [`OSK_SPECIAL`] keys follows them.
pub const OSK_ROWS: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl'", "zxcvbnm,.-"];
/// Its last row.
pub const OSK_SPECIAL: [OskKey; 5] = [
    OskKey::Shift,
    OskKey::Space,
    OskKey::Back,
    OskKey::Enter,
    OskKey::Done,
];

/// The on-screen keyboard: the key the d-pad has picked, and whether letters are shifted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Osk {
    pub row: usize,
    pub col: usize,
    pub shift: bool,
}

impl Osk {
    /// How many keys row `row` has.
    #[must_use]
    pub fn row_len(row: usize) -> usize {
        OSK_ROWS
            .get(row)
            .map_or(OSK_SPECIAL.len(), |r| r.chars().count())
    }

    /// The key at `row`, `col`, as `shift` has it.
    #[must_use]
    pub fn key_at(row: usize, col: usize, shift: bool) -> OskKey {
        match OSK_ROWS.get(row) {
            Some(r) => {
                let c = r.chars().nth(col).unwrap_or(' ');
                let c = if shift {
                    match c {
                        '\'' => '"',
                        ',' => '!',
                        '.' => '?',
                        '-' => '_',
                        c => c.to_ascii_uppercase(),
                    }
                } else {
                    c
                };
                OskKey::Char(c)
            }
            None => OSK_SPECIAL.get(col).copied().unwrap_or(OskKey::Done),
        }
    }

    /// The key picked.
    #[must_use]
    pub fn key(&self) -> OskKey {
        Self::key_at(self.row, self.col, self.shift)
    }

    /// Move the pick a key `dir`, round at the edges; between rows of different lengths it keeps
    /// its place along the row.
    pub fn step(&mut self, dir: Dir) {
        let rows = OSK_ROWS.len() + 1;
        let along = |col: usize, from: usize, to: usize| -> usize {
            let (f, t) = (Self::row_len(from), Self::row_len(to));
            ((col * 2 + 1) * t / (f * 2)).min(t - 1)
        };
        match dir {
            Dir::Left => {
                let n = Self::row_len(self.row);
                self.col = (self.col + n - 1) % n;
            }
            Dir::Right => self.col = (self.col + 1) % Self::row_len(self.row),
            Dir::Up => {
                let to = (self.row + rows - 1) % rows;
                self.col = along(self.col, self.row, to);
                self.row = to;
            }
            Dir::Down => {
                let to = (self.row + 1) % rows;
                self.col = along(self.col, self.row, to);
                self.row = to;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own pad support)
    use super::*;

    #[test]
    fn the_on_screen_keyboard_moves_round_its_keys_and_shifts_its_letters() {
        let mut k = Osk::default();
        assert_eq!(k.key(), OskKey::Char('1'));
        k.step(Dir::Left);
        assert_eq!(k.key(), OskKey::Char('0'), "round the row's end");
        k.step(Dir::Down);
        assert_eq!(k.key(), OskKey::Char('p'));
        k.shift = true;
        assert_eq!(k.key(), OskKey::Char('P'));
        k.step(Dir::Up);
        k.step(Dir::Up);
        assert_eq!(k.row, OSK_ROWS.len(), "round the top to the last row");
        assert_eq!(k.key(), OskKey::Done, "the last row's end stays at its end");
        k.step(Dir::Left);
        k.step(Dir::Left);
        k.step(Dir::Left);
        assert_eq!(k.key(), OskKey::Space);
        k.step(Dir::Up);
        assert!(matches!(k.key(), OskKey::Char(_)));
        assert_eq!(Osk::key_at(3, 8, true), OskKey::Char('?'));
    }
}
