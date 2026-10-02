//! The classic interface's service panels: social, commerce, world, books, assistance and
//! settings.
use super::*;
use crate::int::i32_from;
use dereth_client_contract::view::{BookView, PlayerOption};

mod assistance;
mod book;
mod commerce;
mod settings;
mod social;
mod world;

pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    match id {
        "book" => Some(Box::new(book::Book::default())),
        "social" | "allegiance" | "fellowship" | "trade-intro" | "friends" | "squelch" => {
            social::make(id)
        }
        "trade" | "vendor" | "salvage" => commerce::make(id),
        "map" | "house" | "map-house" | "maintenance" | "game-center" | "link-status" => {
            world::make(id)
        }
        "abuse" | "urgent-assistance" => assistance::make(id),
        "options" | "general-options" | "sound-graphics" => settings::make(id),
        _ => None,
    }
}

const INK: u32 = 0xffd2d2c8;
fn request(r: UiRequest) -> Vec<PanelAction> {
    vec![PanelAction::Game(r)]
}
fn image_button(f: &mut PanelFrame, id: &str, r: Rect, art: [u32; 3], enabled: bool) {
    f.button(id, r, "", enabled).images = Some(art.map(|x| format!("{x:08X}")));
}
fn social_button(f: &mut PanelFrame, id: &str, r: Rect, text: impl Into<String>, enabled: bool) {
    let short = r.h == 27;
    let c = f.button(id, r, text, enabled);
    c.images = Some(
        if short {
            ["06002D3C", "06002D3D", "06002D3B"]
        } else {
            ["06001AB2", "06001AB4", "06001AB0"]
        }
        .map(String::from),
    );
    c.endcaps = Some(
        if short {
            ["06002D39", "06002D38", "06002D3A"]
        } else {
            ["06001AB3", "06001AB5", "06001AB1"]
        }
        .map(String::from),
    );
}
fn label(f: &mut PanelFrame, r: Rect, text: impl Into<String>, font: &str) {
    label_color(f, r, text, font, INK);
}
fn label_color(f: &mut PanelFrame, r: Rect, text: impl Into<String>, font: &str, color: u32) {
    // Inset from the sides only: a line as tall as its box keeps all of its glyphs.
    f.text_box(
        rect(r.x + 2, r.y, (r.w - 4).max(0), r.h),
        text,
        font,
        color,
        TextAlign::Left,
        true,
        Some([r.x, r.y, r.x + r.w, r.y + r.h]),
    );
}
fn centered(f: &mut PanelFrame, r: Rect, text: impl Into<String>, font: &str) {
    f.text_box(
        rect(r.x + 2, r.y + 2, (r.w - 4).max(0), (r.h - 4).max(0)),
        text,
        font,
        INK,
        TextAlign::Center,
        true,
        Some([r.x, r.y, r.x + r.w, r.y + r.h]),
    );
}
fn tiled(width: u32, height: u32, did: &str) -> PanelFrame {
    let mut f = PanelFrame::new(width, height);
    f.image(did, rect(0, 0, width as i32, height as i32), true, false);
    f
}
fn header(f: &mut PanelFrame, title: &str) {
    f.image("0600127B", rect(0, 0, 276, 30), false, false);
    centered(f, rect(0, 0, 276, 25), title, "16-7");
    image_button(
        f,
        "close",
        rect(276, 0, 24, 25),
        [0x06001393, 0x06001394, 0x06001393],
        true,
    );
}
fn translated(mut f: PanelFrame, dy: i32, height: u32) -> PanelFrame {
    f.screen.height = height;
    f.screen.commands = f
        .screen
        .commands
        .into_iter()
        .map(|c| crate::desktop::translate_command(c, 0, dy))
        .collect();
    for c in &mut f.controls {
        c.rect.y += dy;
    }
    for p in &mut f.previews {
        p.rect.y += dy;
    }
    f
}
fn changed_option(option: PlayerOption, checked: bool) -> Vec<PanelAction> {
    vec![
        PanelAction::Game(UiRequest::SetPlayerOption(option, checked)),
        PanelAction::Game(UiRequest::SavePlayerOptions),
    ]
}

fn horizontal_scroll(f: &mut PanelFrame, id: &str, r: Rect, count: usize, offset: i32) {
    let max = (i32_from(count) * 32 - r.w).max(0);
    f.control(
        format!("{id}-scroll"),
        rect(r.x, r.y + r.h, r.w, 16),
        ControlKind::ScrollBar {
            min: 0,
            max,
            value: offset.clamp(0, max),
            page: r.w,
            step: 32,
            vertical: false,
            arrow_size: 16,
            thumb_size: 16,
        },
        max > 0,
    );
}

#[cfg(test)]
mod tests;
