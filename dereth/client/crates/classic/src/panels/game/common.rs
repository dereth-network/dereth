use super::super::{rect, Control, ControlKind, PanelFrame};
use super::super::{DataId, TextAlign};
use crate::{widgets::Rect, Command};
pub const CREAM: u32 = 0xffd2d2c8;
pub const DARK: u32 = 0xff080808;
pub fn art(control: &mut Control, normal: u32, pressed: u32, disabled: u32) {
    control.images = Some([
        format!("{normal:08X}"),
        format!("{pressed:08X}"),
        format!("{disabled:08X}"),
    ]);
}
pub fn close(frame: &mut PanelFrame, normal: u32, pressed: u32) {
    art(
        frame.button("close", rect(276, 0, 24, 25), "", true),
        normal,
        pressed,
        normal,
    );
}
pub fn image(
    frame: &mut PanelFrame,
    did: u32,
    r: Rect,
    clip: Option<[i32; 4]>,
    tile: bool,
    keyed: bool,
) {
    frame.screen.commands.push(Command::Image {
        did: format!("{did:08X}"),
        x: r.x,
        y: r.y,
        width: r.w.max(0) as u32,
        height: r.h.max(0) as u32,
        clip,
        tile,
        color_key: keyed.then_some([0, 0, 0]),
        key_bits: keyed.then_some([5, 6, 5]),
    });
}
/// A spell component's icon on its own, as the item pictures draw it: the icon with no kind
/// background, its edge taking the no-enchantment colour rather than showing as a white outline.
pub fn component_icon(frame: &mut PanelFrame, icon: u32, r: Rect, clip: Option<[i32; 4]>) {
    frame.screen.commands.push(Command::ItemIcon {
        recipe: crate::item_art::Recipe {
            background: None,
            underlay: None,
            icon: Some(icon),
            overlay: None,
            effects: crate::item_art::effect_surface(0),
            badge: None,
        },
        x: r.x,
        y: r.y,
        width: r.w.max(0) as u32,
        height: r.h.max(0) as u32,
        clip,
    });
}
#[allow(clippy::too_many_arguments)] // one field per argument of the drawn command
pub fn text(
    frame: &mut PanelFrame,
    r: Rect,
    value: impl Into<String>,
    font: &str,
    color: u32,
    align: u8,
    wrap: bool,
    clip: Option<[i32; 4]>,
) {
    frame.text_box(
        r,
        value,
        font,
        color,
        match align {
            1 => super::super::TextAlign::Center,
            2 => super::super::TextAlign::Right,
            _ => super::super::TextAlign::Left,
        },
        wrap,
        clip,
    );
}
pub fn list_hits(
    frame: &mut PanelFrame,
    id: &str,
    r: Rect,
    count: usize,
    row_height: i32,
    selected: Option<usize>,
    offset: i32,
) {
    frame.control(
        id,
        r,
        ControlKind::HitList {
            row_count: count,
            row_height,
            selected,
            offset,
        },
        true,
    );
}
pub fn number(value: impl ToString) -> String {
    let value = value.to_string();
    let (sign, digits) = value
        .strip_prefix('-')
        .map_or(("", value.as_str()), |v| ("-", v));
    let mut result = String::from(sign);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}
pub fn panel_backdrop(frame: &mut PanelFrame, height: i32) {
    image(
        frame,
        0x0600128a,
        rect(4, 25, 292, height - 29),
        None,
        true,
        false,
    );
    image(
        frame,
        0x060012bc,
        rect(0, 25, 4, height - 25),
        None,
        true,
        false,
    );
    image(
        frame,
        0x060012bd,
        rect(296, 25, 4, height - 25),
        None,
        true,
        false,
    );
    image(
        frame,
        0x060012bb,
        rect(4, height - 4, 292, 4),
        None,
        true,
        false,
    );
}

#[allow(clippy::too_many_arguments)] // one field per argument of the scroll bar control
pub fn scrollbar(
    frame: &mut PanelFrame,
    id: &str,
    r: Rect,
    content: i32,
    page: i32,
    value: i32,
    step: i32,
    vertical: bool,
) {
    let narrow = if vertical { r.w <= 16 } else { r.h <= 16 };
    frame.control(
        id,
        r,
        ControlKind::ScrollBar {
            min: 0,
            max: (content - page).max(0),
            value: value.clamp(0, (content - page).max(0)),
            page,
            step,
            vertical,
            arrow_size: if narrow { 16 } else { 20 },
            thumb_size: if narrow { 16 } else { 28 },
        },
        true,
    );
}

/// A spell icon: level art, the icon through its white mask, and self/fellowship badges.
pub fn spell_icon(
    f: &mut PanelFrame,
    icon: Option<DataId>,
    power: u32,
    bitfield: u32,
    r: Rect,
    clip: Option<[i32; 4]>,
) {
    if let Some(icon) = icon {
        f.screen.commands.push(crate::Command::SpellIcon {
            icon: icon.0,
            power,
            bitfield,
            x: r.x,
            y: r.y,
            width: r.w as u32,
            height: r.h as u32,
            clip,
        });
    }
}
/// Measure with the same installed atlas used to draw. Empty/headless frames have no scroll.
pub fn rich_scroll(
    f: &mut PanelFrame,
    r: Rect,
    runs: Vec<crate::TextRun>,
    font: &str,
    scroll: i32,
) {
    rich_scroll_inset(f, r, 0, runs, font, scroll);
}
/// [`rich_scroll`] with the text kept `inset` pixels inside the box's edges.
pub fn rich_scroll_inset(
    f: &mut PanelFrame,
    r: Rect,
    inset: i32,
    runs: Vec<crate::TextRun>,
    font: &str,
    scroll: i32,
) {
    let height = crate::renderer::measure_rich_text_height(font, &runs, r.w - 2 * inset)
        .unwrap_or(r.h)
        .max(r.h);
    let offset = scroll.clamp(0, (height - r.h).max(0));
    f.rich_text_box(
        rect(r.x + inset, r.y + inset - offset, r.w - 2 * inset, height),
        runs,
        font,
        TextAlign::Left,
        true,
        Some([r.x, r.y, r.x + r.w, r.y + r.h]),
    );
    scrollbar(
        f,
        "description",
        rect(r.x + r.w, r.y, 19, r.h),
        height,
        r.h,
        offset,
        15,
        true,
    );
}
