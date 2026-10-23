//! A generator explained in plain words, as the inspector shows a selected spawn that is one:
//! what it makes (each entry, by name, with its picture), how many at once, where, how soon it
//! makes more, and what starts it. Every line names the server's field it reads in its tooltip.
//! A server's own generator is read from the server's database, as it is: nothing is copied.

use dereth_primitives::DataId;
use lyceum_foundry::content::SqlDoc;
use lyceum_foundry::edit::{generator_of, Generator, Profile, Spawn, Weenie};

use super::EditorApp;

/// Each entry's share of the draw, in percent, from the running totals the profiles write
/// (`None` for an entry made always, written `-1`). A total lower than the one before it starts
/// the count again, as the server reads them.
#[must_use]
pub fn shares(profiles: &[Profile]) -> Vec<Option<f32>> {
    let mut last = 0.0f32;
    let mut raw = Vec::with_capacity(profiles.len());
    for p in profiles {
        match p.probability {
            Some(v) if v >= 0.0 => {
                if v < last {
                    last = 0.0;
                }
                raw.push(Some(v - last));
                last = v;
            }
            _ => raw.push(None),
        }
    }
    let total: f32 = raw.iter().flatten().sum();
    raw.into_iter()
        .map(|s| s.map(|s| if total > 0.0 { s / total * 100.0 } else { 0.0 }))
        .collect()
}

/// The keys of the words for where an entry is made, by the bits of `where_Create`.
fn places(place: u32) -> Vec<&'static str> {
    [
        (0x01, "generator.on_top"),
        (0x02, "generator.scatter"),
        (0x04, "generator.specific"),
        (0x08, "generator.contain"),
        (0x10, "generator.wield"),
        (0x20, "generator.shop"),
        (0x40, "generator.treasure"),
    ]
    .into_iter()
    .filter(|(bit, _)| place & bit != 0)
    .map(|(_, key)| key)
    .collect()
}

/// The key of the words for what lets an entry be made again, by `when_Create`.
fn when(when: u32) -> &'static str {
    match when {
        1 => "generator.when_destroyed",
        2 => "generator.when_picked_up",
        4 => "generator.when_killed",
        _ => "generator.when_gone",
    }
}

impl EditorApp {
    /// The generator spawn `s` is, its entries read from the server's class when no pack writes
    /// it (`None` while that is being read, or for a spawn that is no generator).
    pub fn spawn_generator(&mut self, s: &Spawn) -> Option<Generator> {
        let mut g = self.session.generator(s);
        if g.as_ref().is_none_or(|g| g.profiles.is_empty()) {
            if let Some(Weenie::Class(c)) = self.session.spawn_weenie(s) {
                if self.session.weenie_file(&Weenie::Class(c)).is_none() {
                    if let Some(Some(text)) = self.server_class(c) {
                        let mut full = generator_of(&SqlDoc::parse(&text));
                        if let Some(known) = g {
                            full.children = known.children;
                            full.radius = full.radius.or(known.radius);
                        }
                        g = Some(full);
                    }
                }
            }
        }
        g.filter(|g| g.radius.is_some() || !g.profiles.is_empty() || !g.children.is_empty())
    }

    /// The inspector's account of generator `g`.
    pub(super) fn generator_card(&mut self, ui: &mut egui::Ui, g: &Generator) {
        let t = self.text.table();
        ui.separator();
        ui.strong(t.get("generator.title"));
        ui.label(t.get("generator.about"));
        let tip = |key: &str| t.get(key).to_owned();
        // How many, all told.
        if g.most.is_some() || g.first.is_some() {
            let most = g
                .most
                .map_or_else(|| t.get("generator.no_limit").to_owned(), |n| n.to_string());
            let first = g.first.unwrap_or(0);
            ui.label(t.fill("generator.how_many", &[("most", &most), ("first", &first)]))
                .on_hover_text(tip("generator.how_many_tip"));
        }
        if let Some(r) = g.radius {
            ui.label(t.fill("generator.radius", &[("m", &format!("{r:.1}"))]))
                .on_hover_text(tip("generator.radius_tip"));
        }
        if let Some(s) = g.interval {
            ui.label(t.fill("generator.refill", &[("s", &format!("{s:.0}"))]))
                .on_hover_text(tip("generator.refill_tip"));
        }
        let start = match g.initial_delay.filter(|d| *d > 0.0) {
            Some(d) => t.fill("generator.start_after", &[("s", &format!("{d:.0}"))]),
            None => t.get("generator.start").to_owned(),
        };
        ui.label(start).on_hover_text(tip("generator.start_tip"));
        if let Some(key) = match g.time {
            Some(2) => Some("generator.time_defined"),
            Some(3) => Some("generator.time_event"),
            Some(4) => Some("generator.time_night"),
            Some(5) => Some("generator.time_day"),
            _ => None,
        } {
            ui.label(t.get(key))
                .on_hover_text(tip("generator.time_tip"));
        }
        if !g.children.is_empty() {
            ui.label(t.fill("generator.owns", &[("n", &g.children.len())]));
        }
        // What it makes.
        if g.profiles.is_empty() {
            return;
        }
        ui.label(t.fill("generator.makes", &[("n", &g.profiles.len())]));
        let shares = shares(&g.profiles);
        let mut budget = super::PICTURES_A_FRAME;
        for (i, p) in g.profiles.iter().enumerate() {
            let class = lyceum_foundry::sql_ids::number(&p.class);
            let name = self.class_name(&p.class).unwrap_or_else(|| p.class.clone());
            let setup = class
                .and_then(|c| self.session.class_facts(c))
                .and_then(|f| f.setup)
                .or_else(|| {
                    self.weenies
                        .iter()
                        .find(|w| w.name == name)
                        .and_then(|w| w.setup)
                });
            ui.push_id(("profile", i), |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(id) = setup.and_then(|s| self.picture(DataId(s), &mut budget)) {
                            ui.add(egui::Image::new((id, egui::vec2(40.0, 40.0))));
                        }
                        ui.vertical(|ui| {
                            ui.strong(&name);
                            let most = if p.max < 0 {
                                t.get("generator.no_limit").to_owned()
                            } else {
                                p.max.to_string()
                            };
                            ui.label(
                                t.fill("generator.count", &[("first", &p.init), ("most", &most)]),
                            )
                            .on_hover_text(tip("generator.count_tip"));
                            let wheres: Vec<String> = places(p.place)
                                .into_iter()
                                .map(|k| {
                                    t.fill(k, &[("m", &format!("{:.1}", g.radius.unwrap_or(0.0)))])
                                })
                                .collect();
                            if !wheres.is_empty() {
                                ui.label(wheres.join(", "))
                                    .on_hover_text(tip("generator.place_tip"));
                            }
                            let again = match p.delay.filter(|d| *d > 0.0) {
                                Some(d) => t.fill(
                                    "generator.again_after",
                                    &[("when", &t.get(when(p.when))), ("s", &format!("{d:.0}"))],
                                ),
                                None => {
                                    t.fill("generator.again", &[("when", &t.get(when(p.when)))])
                                }
                            };
                            ui.label(again).on_hover_text(tip("generator.again_tip"));
                            match shares[i] {
                                Some(share) => {
                                    ui.weak(t.fill(
                                        "generator.chance",
                                        &[("n", &format!("{share:.0}"))],
                                    ))
                                    .on_hover_text(tip("generator.chance_tip"));
                                }
                                None => {
                                    ui.weak(t.get("generator.always"))
                                        .on_hover_text(tip("generator.chance_tip"));
                                }
                            }
                        });
                    });
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a generator explained in the world editor)
    use super::*;

    fn profile(p: Option<f32>) -> Profile {
        Profile {
            class: "1".into(),
            init: 1,
            max: 1,
            place: 2,
            cell: None,
            origin: None,
            when: 1,
            delay: None,
            probability: p,
        }
    }

    #[test]
    fn running_totals_become_each_entrys_share_and_an_always_entry_has_none() {
        let s = shares(&[profile(Some(0.25)), profile(Some(1.0)), profile(Some(-1.0))]);
        assert_eq!(s.len(), 3);
        assert!((s[0].unwrap() - 25.0).abs() < 1e-3);
        assert!((s[1].unwrap() - 75.0).abs() < 1e-3);
        assert_eq!(s[2], None);
        // A total lower than the one before starts again.
        let s = shares(&[profile(Some(0.5)), profile(Some(0.5))]);
        assert!((s[0].unwrap() - 100.0).abs() < 1e-3 && s[1].unwrap().abs() < 1e-3);
    }

    #[test]
    fn where_and_when_are_told_by_their_bits() {
        assert_eq!(
            places(0x02 | 0x40),
            ["generator.scatter", "generator.treasure"]
        );
        assert_eq!(when(4), "generator.when_killed");
        assert_eq!(when(0), "generator.when_gone");
    }
}
