//! What storekeepers sell: each storekeeper template's store, what it greets a shopper with, and
//! each store's sales in order (the product, its price and currency, how many to a stack, the icon
//! its listing shows, its name). Every row says where it came from.
//!
//! A sale may be for some players only (a race, a least level, quests at a status); a store either
//! hides the sales a player may not have or shows them restricted.
//!
//! The text form is tab-separated, one row per line, the first column saying which kind of row:
//! `keeper` (template, store, greeting as `string:table`, `=` and literal text or empty,
//! provenance), `sale` (store,
//! ordinal, product, cost, currency, stack, icon layers as `layer:picture:color` joined by spaces,
//! name string, name table, provenance, and optionally who it is for: `race=N`, `level=N`,
//! `quest=QUEST:STATUS` joined by spaces) or `hides` (a store that hides what a player may not
//! have).

use std::fmt::Write as _;

use crate::talk::Words;

/// A storekeeper: the store it opens and its greeting (none: it opens its store without a word).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keeper {
    pub template: u32,
    pub store: u32,
    pub greeting: Option<Words>,
    pub placed: String,
}

/// One sale of a store.
#[derive(Debug, Clone, PartialEq)]
pub struct Sale {
    pub store: u32,
    pub ordinal: u32,
    pub product: u32,
    pub cost: f32,
    pub currency: u32,
    pub stack: u32,
    /// The listing's icon: layer, picture, color.
    pub icon: Vec<(u32, u32, u32)>,
    pub name: (u32, u32),
    pub placed: String,
    /// Who it is for.
    pub gate: SaleGate,
}

/// Who a sale is for: a player of the race (0: any), of the least level (0: any), holding each
/// quest at its status (status 0: holding the quest at all).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaleGate {
    pub race: u32,
    pub level: u32,
    pub quests: Vec<(u32, u32)>,
}

impl SaleGate {
    /// Whether a player may have the sale: its race, its level, and its quests with their status.
    #[must_use]
    pub fn admits(&self, race: u32, level: i32, quests: &dyn Fn(u32) -> Option<u32>) -> bool {
        (self.level == 0 || i64::from(level) >= i64::from(self.level))
            && (self.race == 0 || race == self.race)
            && self
                .quests
                .iter()
                .all(|(q, want)| quests(*q).is_some_and(|st| *want == 0 || st == *want))
    }

    fn to_text(&self) -> String {
        let mut out = Vec::new();
        if self.race != 0 {
            out.push(format!("race={}", self.race));
        }
        if self.level != 0 {
            out.push(format!("level={}", self.level));
        }
        for (q, st) in &self.quests {
            out.push(format!("quest={q:08X}:{st:08X}"));
        }
        out.join(" ")
    }

    fn from_text(s: &str) -> Result<Self, String> {
        let mut g = Self::default();
        for part in s.split(' ').filter(|x| !x.is_empty()) {
            match part.split_once('=') {
                Some(("race", v)) => g.race = v.parse().map_err(|e| format!("{part}: {e}"))?,
                Some(("level", v)) => g.level = v.parse().map_err(|e| format!("{part}: {e}"))?,
                Some(("quest", v)) => {
                    let (q, st) = v
                        .split_once(':')
                        .ok_or_else(|| format!("{part}: QUEST:STATUS"))?;
                    g.quests.push((hex(q)?, hex(st)?));
                }
                _ => return Err(format!("{part} is not race=, level= or quest=")),
            }
        }
        Ok(g)
    }
}

/// Every storekeeper and every sale.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoreTable {
    pub keepers: Vec<Keeper>,
    pub sales: Vec<Sale>,
    /// The stores that hide the sales a player may not have (the others show them restricted).
    pub hiding: Vec<u32>,
}

fn hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

impl StoreTable {
    /// The storekeeper of a template.
    #[must_use]
    pub fn keeper(&self, template: u32) -> Option<&Keeper> {
        self.keepers.iter().find(|k| k.template == template)
    }

    /// Whether a store hides the sales a player may not have.
    #[must_use]
    pub fn hides(&self, store: u32) -> bool {
        self.hiding.contains(&store)
    }

    /// A store's sales, in order.
    #[must_use]
    pub fn sales_of(&self, store: u32) -> Vec<&Sale> {
        let mut v: Vec<&Sale> = self.sales.iter().filter(|s| s.store == store).collect();
        v.sort_by_key(|s| s.ordinal);
        v
    }

    /// The table as text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::from("# kind\tfields...\n");
        for k in &self.keepers {
            let _ = writeln!(
                out,
                "keeper\t{:08X}\t{:08X}\t{}\t{}",
                k.template,
                k.store,
                k.greeting.as_ref().map(Words::to_text).unwrap_or_default(),
                k.placed
            );
        }
        for s in &self.sales {
            let icon = s
                .icon
                .iter()
                .map(|(l, p, c)| format!("{l}:{p:08X}:{c:08X}"))
                .collect::<Vec<_>>()
                .join(" ");
            let gate = s.gate.to_text();
            let _ = writeln!(
                out,
                "sale\t{:08X}\t{}\t{:08X}\t{}\t{:08X}\t{}\t{icon}\t{:08X}\t{:08X}\t{}{}{gate}",
                s.store,
                s.ordinal,
                s.product,
                s.cost,
                s.currency,
                s.stack,
                s.name.0,
                s.name.1,
                s.placed,
                if gate.is_empty() { "" } else { "\t" },
            );
        }
        for h in &self.hiding {
            let _ = writeln!(out, "hides\t{h:08X}");
        }
        out
    }

    /// Read a table's text.
    ///
    /// # Errors
    /// A row without its columns, or a value that does not read.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut t = Self::default();
        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let at = |e: String| format!("line {}: {e}", n + 1);
            let c: Vec<&str> = line.split('\t').collect();
            match c[0] {
                "keeper" if c.len() == 5 => t.keepers.push(Keeper {
                    template: hex(c[1]).map_err(at)?,
                    store: hex(c[2]).map_err(at)?,
                    greeting: if c[3].is_empty() {
                        None
                    } else {
                        Some(Words::from_text(c[3]).map_err(at)?)
                    },
                    placed: c[4].to_string(),
                }),
                "hides" if c.len() == 2 => t.hiding.push(hex(c[1]).map_err(at)?),
                "sale" if c.len() == 11 || c.len() == 12 => {
                    let icon = c[7]
                        .split(' ')
                        .filter(|x| !x.is_empty())
                        .map(|x| {
                            let p: Vec<&str> = x.split(':').collect();
                            if p.len() != 3 {
                                return Err(format!(
                                    "an icon layer is layer:picture:color, not {x}"
                                ));
                            }
                            Ok((
                                p[0].parse::<u32>().map_err(|e| e.to_string())?,
                                hex(p[1])?,
                                hex(p[2])?,
                            ))
                        })
                        .collect::<Result<Vec<_>, String>>()
                        .map_err(at)?;
                    t.sales.push(Sale {
                        store: hex(c[1]).map_err(at)?,
                        ordinal: c[2].parse().map_err(|e| at(format!("{e}")))?,
                        product: hex(c[3]).map_err(at)?,
                        cost: c[4].parse().map_err(|e| at(format!("{e}")))?,
                        currency: hex(c[5]).map_err(at)?,
                        stack: c[6].parse().map_err(|e| at(format!("{e}")))?,
                        icon,
                        name: (hex(c[8]).map_err(at)?, hex(c[9]).map_err(at)?),
                        placed: c[10].to_string(),
                        gate: match c.get(11) {
                            Some(g) => SaleGate::from_text(g).map_err(at)?,
                            None => SaleGate::default(),
                        },
                    });
                }
                other => {
                    return Err(at(format!(
                        "a row of kind {other} with {} columns",
                        c.len()
                    )))
                }
            }
        }
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the table's own text form, read back)
    use super::*;

    #[test]
    fn a_sales_gate_and_a_hiding_store_read_back_as_written() {
        let sale = |ordinal: u32, gate: SaleGate| Sale {
            store: 0x6F00_16ED,
            ordinal,
            product: 0x4700_18B5,
            cost: 500.0,
            currency: 0x4700_0538,
            stack: 1,
            icon: Vec::new(),
            name: (0xA860_7CEC, 0x2500_1761),
            placed: "authored for a test".into(),
            gate,
        };
        let t = StoreTable {
            keepers: Vec::new(),
            sales: vec![
                sale(0, SaleGate::default()),
                sale(
                    1,
                    SaleGate {
                        race: 2,
                        level: 10,
                        quests: vec![(0x8000_0342, 0x3000_0000), (0x8000_0290, 0)],
                    },
                ),
            ],
            hiding: vec![0x6F00_16ED],
        };
        assert_eq!(StoreTable::from_text(&t.to_text()).expect("reads"), t);
        assert!(StoreTable::from_text(
            "sale\t6F0016ED\t0\t470018B5\t500\t47000538\t1\t\tA8607CEC\t25001761\tx\trace=x"
        )
        .is_err());
    }
}
