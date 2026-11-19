//! The view as text: what `snapshot` and `position` print.
//!
//! One fact a line, a list's members indented under it, numbers as the view holds them; an object
//! by its id, a name the view has only as a string reference shown as `?`.

use auberean_client_sdk::contract::view::View;
use auberean_client_sdk::Phase;

/// Where the player's body stands: its cell, its place there and its heading, and whether it
/// moves; `none` before the player is here.
#[must_use]
pub fn position(view: &View) -> String {
    view.player.as_ref().map_or_else(
        || "none".to_owned(),
        |p| {
            let b = p.body;
            format!(
                "{:08X} ({:.2}, {:.2}, {:.2}) facing {:.0}{}",
                b.cell,
                b.origin[0],
                b.origin[1],
                b.origin[2],
                // Rounded first, so a heading just under nothing reads 0, not -0.
                b.heading.round() + 0.0,
                if b.moving { " moving" } else { "" }
            )
        },
    )
}

/// The view, a line each fact.
#[must_use]
pub fn text(view: &View, phase: Phase) -> Vec<String> {
    let mut out = vec![format!("phase: {phase:?}")];
    if let Some(list) = &view.pregame.characters {
        out.push(format!(
            "characters: {} of {} on {}",
            list.characters.len(),
            list.slots,
            list.account
        ));
        for c in &list.characters {
            out.push(format!("  {:#018x} {}", c.id, c.name));
        }
    }
    if let Some(p) = &view.player {
        out.push(format!(
            "player: {:#018x} {} on {}",
            p.id,
            p.name.as_deref().unwrap_or("?"),
            p.world_name
        ));
        if let Some([health, most_health, vigor, most_vigor]) = p.vitals {
            out.push(format!(
                "vitals: health {health}/{most_health}, vigor {vigor}/{most_vigor}"
            ));
        }
    }
    out.push(format!("position: {}", position(view)));
    if let Some(w) = &view.world {
        let s = w.selection;
        out.push(format!(
            "selection: {:#018x}, before it {:#018x}",
            s.current, s.last
        ));
        out.push(format!("around: {}", w.things.len()));
        for t in &w.things {
            let at = t.place.map_or_else(
                || {
                    t.parent
                        .map_or_else(String::new, |p| format!(" on {p:#018x}"))
                },
                |(cell, o)| format!(" at {cell:08X} ({:.1}, {:.1}, {:.1})", o[0], o[1], o[2]),
            );
            out.push(format!(
                "  {:#018x} {}{at}{}{}",
                t.id,
                t.name.as_deref().unwrap_or("?"),
                if t.selectable { " selectable" } else { "" },
                if t.dead { " dead" } else { "" },
            ));
        }
    }
    if let Some(i) = &view.inventory {
        out.push(format!(
            "inventory: {} items, {} money",
            i.items.len(),
            i.money
        ));
        for (id, item) in &i.items {
            out.push(format!(
                "  {id:#018x} {} x{}{}",
                item.name,
                item.quantity,
                if item.worn { " worn" } else { "" }
            ));
        }
    }
    if let Some(e) = &view.effects {
        out.push(format!("effects: {}", e.effects.len()));
        for e in &e.effects {
            out.push(format!("  {} {}", e.id, e.name));
        }
    }
    if !view.questions.is_empty() {
        out.push(format!("questions: {}", view.questions.len()));
        for q in &view.questions {
            out.push(format!(
                "  {} {:?}{}",
                q.id,
                q.asked,
                if q.at_once { " first" } else { "" }
            ));
        }
    }
    out.push(format!(
        "calls: {} run, {} to the end",
        view.calls.run, view.calls.completed
    ));
    out
}
