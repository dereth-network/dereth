//! An overlay's iterations, numbered from its own content: its generations.
//!
//! A client keeps a world's records as iterations over its own files, and is sent the world's
//! records when the world has iterations it lacks. An overlay numbered only from the files it lies
//! over keeps the same iteration however its records change, so a client holding an older one of
//! the world is never sent the newer one.
//!
//! The iterations a file holds must run on unbroken (the iteration record is a run of consecutive
//! numbers, and a server reads a gap as a file it cannot trust), and a newer overlay must hold more
//! of them than an older one, or a client holding the older is refused as holding a newer world.
//! So a digest of everything the overlay holds ([`digest`]) is looked up in a ledger of the
//! contents written ([`Ledger`], kept as `generations.tsv`), each with its generation, in the order
//! they were first made: the overlay holds as many iterations past its base files' as its
//! generation ([`iterations`]). The same content always takes the same generation; new content
//! takes the next one, and is added to the ledger.

use sha2::{Digest, Sha256};

/// A digest of the records an overlay writes, in the order given: each record's id, its length and
/// its bytes, so no two different sets of records read alike.
#[must_use]
pub fn digest<'a>(records: impl IntoIterator<Item = (u32, &'a [u8])>) -> [u8; 32] {
    let mut h = Sha256::new();
    for (id, bytes) in records {
        h.update(id.to_le_bytes());
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    h.finalize().into()
}

/// A digest written as hex.
#[must_use]
pub fn hex(d: &[u8; 32]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The ledger of the contents written: each digest with its generation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    rows: Vec<(String, u32)>,
}

impl Ledger {
    /// The ledger from its file's text: a header line, then `digest<TAB>generation` a line.
    ///
    /// # Errors
    /// A line that is not a digest and a number.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut rows = Vec::new();
        for (n, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let (d, g) = line
                .split_once('\t')
                .ok_or_else(|| format!("generations line {}: no tab", n + 1))?;
            let g: u32 = g
                .trim()
                .parse()
                .map_err(|e| format!("generations line {}: {e}", n + 1))?;
            rows.push((d.trim().to_owned(), g));
        }
        Ok(Self { rows })
    }

    /// The ledger as its file holds it.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = String::from("digest\tgeneration\n");
        for (d, g) in &self.rows {
            s.push_str(&format!("{d}\t{g}\n"));
        }
        s
    }

    /// The generation of the content with digest `d`, and whether it is new: the one it already
    /// has, or the next after the last, added.
    pub fn generation(&mut self, d: &[u8; 32]) -> (u32, bool) {
        let h = hex(d);
        if let Some((_, g)) = self.rows.iter().find(|(r, _)| *r == h) {
            return (*g, false);
        }
        let g = self.rows.iter().map(|(_, g)| *g).max().unwrap_or(0) + 1;
        self.rows.push((h, g));
        (g, true)
    }
}

/// The iterations an overlay of generation `g` holds over files whose own iterations end below
/// `over`: `over`, and on, `g` of them; its records are in the last.
#[must_use]
pub fn iterations(over: u32, g: u32) -> Vec<u32> {
    (over..over + g.max(1)).collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: an overlay's generations)
    use super::*;

    #[test]
    fn the_same_overlay_takes_the_same_iterations_and_another_overlay_more() {
        let a: Vec<(u32, &[u8])> = vec![(0x0101_FFFF, b"land"), (0x1300_0000, b"region")];
        let b: Vec<(u32, &[u8])> = vec![(0x0101_FFFF, b"land"), (0x1300_0000, b"region!")];
        let mut ledger = Ledger::default();
        let (ga, new_a) = ledger.generation(&digest(a.clone()));
        let (gb, new_b) = ledger.generation(&digest(b));
        let (ga2, again) = ledger.generation(&digest(a));
        assert!(new_a && new_b && !again);
        assert_eq!(ga, ga2);
        assert_ne!(iterations(983, ga), iterations(983, gb));
        assert!(gb > ga, "a newer overlay holds more iterations");
        // And it reads back from its file as it was.
        assert_eq!(Ledger::parse(&ledger.text()).unwrap(), ledger);
    }

    #[test]
    fn a_records_length_is_part_of_the_digest() {
        let a: Vec<(u32, &[u8])> = vec![(1, b"land"), (2, b"region")];
        let c: Vec<(u32, &[u8])> = vec![(1, b"lan"), (2, b"dregion")];
        assert_ne!(digest(a), digest(c));
    }

    #[test]
    fn the_iterations_run_on_unbroken_past_the_bases() {
        assert_eq!(iterations(983, 1), [983]);
        assert_eq!(iterations(983, 3), [983, 984, 985]);
        assert_eq!(iterations(983, 0), [983]);
    }
}
