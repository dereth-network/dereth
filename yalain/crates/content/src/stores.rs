//! What storekeepers sell: each storekeeper template's store, what it greets a shopper with, and
//! each store's sales in order (the product, its price and currency, how many to a stack, the icon
//! its listing shows, its name). Every row says where it came from.
//!
//! The text form is tab-separated, one row per line, the first column saying which kind of row:
//! `keeper` (template, store, greeting as `string:table`, `=` and literal text or empty,
//! provenance) or `sale` (store,
//! ordinal, product, cost, currency, stack, icon layers as `layer:picture:color` joined by spaces,
//! name string, name table, provenance).

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
}

/// Every storekeeper and every sale.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoreTable {
    pub keepers: Vec<Keeper>,
    pub sales: Vec<Sale>,
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
            let _ = writeln!(
                out,
                "sale\t{:08X}\t{}\t{:08X}\t{}\t{:08X}\t{}\t{icon}\t{:08X}\t{:08X}\t{}",
                s.store,
                s.ordinal,
                s.product,
                s.cost,
                s.currency,
                s.stack,
                s.name.0,
                s.name.1,
                s.placed
            );
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
                "sale" if c.len() == 11 => {
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
