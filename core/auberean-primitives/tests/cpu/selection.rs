use std::cell::RefCell;

use auberean_primitives::random::{Random, SelectionTable};

use super::oracle::check;
use super::selection_cases;

/// Objects drawn by weight from a table, each once until the table starts over (when a draw
/// finds nothing left), come out in the September 2013 client's own order from a generator in
/// the state every run starts in, and the table's binary form and its subtree weights worked
/// out again from the weights alone match the client's word for word.
///
/// Behaviour: geometry.selection.weighted-draws-follow-the-client
#[test]
fn weighted_draws_follow_the_client() {
    let random = Random::new();
    let last: RefCell<Option<SelectionTable<u32>>> = RefCell::new(None);
    check(selection_cases::CASES, |label, _, inp| {
        let weights = &inp[1..=inp[0] as usize];
        match label {
            "choose" => {
                let mut t = SelectionTable::default();
                for (k, &w) in (100..).zip(weights) {
                    t.add(k, w);
                }
                let mut out: Vec<u32> = Vec::new();
                for _ in 0..weights.len() * 2 + 1 {
                    if let Some(o) = t.choose(&random) {
                        out.push(o);
                    } else {
                        t.start_over();
                        out.push(u32::MAX);
                    }
                }
                out.extend(
                    t.pack()
                        .chunks(4)
                        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])),
                );
                *last.borrow_mut() = Some(t);
                Some(out)
            }
            "initialize_weights" => {
                let mut t = last.borrow_mut().take()?;
                for e in &mut t.unused {
                    e.subtree = 0;
                }
                if !t.unused.is_empty() {
                    t.initialize_weights(0);
                }
                Some(t.unused.iter().map(|e| e.subtree).collect())
            }
            _ => None,
        }
    });
}
