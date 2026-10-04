//! Saved window placement and chat preferences.

use super::*;

impl Hud {
    /// Consume local numeric placement writes in order, before any PM readback. This is the
    /// host-owned call, not an outbound request.
    /// Missing retained module is this host's current ownership boundary, not a new retail
    /// readiness flag.
    pub fn consume_placement_requests(
        &mut self,
        world: &mut dereth_client_model::World,
        requests: &mut Vec<dereth_client_contract::UiRequest>,
        now: dereth_primitives::ServerTime,
    ) {
        let mut wrote = false;
        requests.retain(|request| {
            if !dereth_client_contract::requests::is_local_module_write(request) {
                return true;
            }
            // The two chat-option arms join the placement arm. All three are one function in the
            // client: setting a chat-window option or a gameplay option raises a local notice and
            // sets the 480-second dirty flag. Nothing here sends.
            match request {
                dereth_client_contract::UiRequest::SetChatWindowOption {
                    window,
                    property,
                    value,
                } => {
                    wrote |= world
                        .player_system
                        .set_chat_window_option(*window, *property, *value, now);
                }
                dereth_client_contract::UiRequest::SetChatWindowTitle { window, title } => {
                    let value = match title {
                        dereth_client_contract::ChatWindowTitle::Literal(text) => {
                            dereth_protocol::property::StringInfo {
                                over: 1,
                                literal: Some(text.clone()),
                                ..Default::default()
                            }
                        }
                        dereth_client_contract::ChatWindowTitle::Table {
                            string_id,
                            table_id,
                        } => dereth_protocol::property::StringInfo {
                            string_id: *string_id,
                            table_id: *table_id,
                            ..Default::default()
                        },
                    };
                    wrote |= world
                        .player_system
                        .set_chat_window_title(*window, value, now);
                }
                dereth_client_contract::UiRequest::SetChatWindowFilter { window, mask } => {
                    wrote |= world
                        .player_system
                        .set_chat_window_filter(*window, *mask, now);
                }
                dereth_client_contract::UiRequest::SetChatOpacity { property, value } => {
                    wrote |= world
                        .player_system
                        .set_gameplay_option_float(*property, *value, now);
                }
                _ => {}
            }
            false
        });
        if !wrote {
            return;
        }
        let module = world
            .player_system
            .module
            .as_ref()
            .expect("accepted local property write");
        self.placements = decode_placements(module);
        if let Some(snapshot) = self.player_module.as_mut() {
            snapshot
                .gameplay_options
                .clone_from(&module.gameplay_options);
        }
        // The retail shared gameplay-option-changed notice ignores placement IDs:
        // local writes must not cause a full screen re-seed on the following frame. A different
        // screen_serial or authoritative PlayerDescription still invalidates the key normally.
        if let Some(key) = self.applied_to.as_mut() {
            key.placements.clone_from(&self.placements);
        }
    }
}

/// `Option_PlacementArray`, the one array property the gameplay-options collection holds.
pub const ARRAY: u32 = 0x1000_008C;
pub const X: u32 = 0x1000_0086;
pub const Y: u32 = 0x1000_0087;
pub const WIDTH: u32 = 0x1000_0088;
pub const HEIGHT: u32 = 0x1000_0089;
pub const VISIBILITY: u32 = 0x1000_008A;
pub const TITLE: u32 = 0x1000_008D;

/// Decode player-module gameplay options into `Option_PlacementArray` rows keyed by window id.
/// Look `0x1000008C`
/// up in the collection, check its descriptor is type `0x11` (`Array`), index it by `windowID − 1`,
/// and read the named property out of the `0x1000008B` struct that lands there.
#[must_use]
pub fn decode_placements(m: &PlayerModule) -> WindowPlacements {
    let mut out = WindowPlacements::default();
    let Some(opts) = m.gameplay_options.as_ref() else {
        return out;
    };
    let Some(BasePropertyValue::Array(rows)) = opts.properties.get(placement::ARRAY) else {
        return out;
    };
    for (i, row) in rows.iter().enumerate() {
        let Some(BasePropertyValue::Struct(fields)) = row.value.as_ref() else {
            continue;
        };
        let window_id = u32::try_from(i).unwrap_or(0) + 1;
        out.set(window_id, placement_row(fields));
    }
    out
}

/// The two **general** chat options the Chat Options panel's sliders write —
/// `0x10000080 Option_DefaultOpacity` and `0x10000081 Option_ActiveOpacity`, both `T::Float` in
/// `dereth_protocol::property::PROPERTY_TYPES`.
///
/// They sit at the **top** of the gameplay-options collection, not inside the `0x1000008C` per-window
/// array `decode_placements` walks: the Chat Options page's general section is one pair of
/// sliders for all five windows, and each request names the property with no window index.
///
/// `None` is a module that carries no value, which is not the same as one that carries `0.0` —
/// the options reader substitutes `1.0f` for an absent attribute; a
/// zero would make the chat window invisible.
#[must_use]
pub fn decode_chat_opacity(m: &PlayerModule) -> (Option<f32>, Option<f32>) {
    let Some(opts) = m.gameplay_options.as_ref() else {
        return (None, None);
    };
    let f = |name: u32| match opts.properties.get(name) {
        Some(BasePropertyValue::Float(v)) => Some(*v),
        _ => None,
    };
    (
        f(dereth_client_contract::chat::interface::opacity_attr::DEFAULT),
        f(dereth_client_contract::chat::interface::opacity_attr::ACTIVE),
    )
}

/// `(window id, 0x1000007F)` for every window the blob carries —
/// the per-window 64-bit text-type filter the Chat Options page edits.
///
/// Same walk as [`decode_placements`], because it is literally the same function:
/// The chat-option lookup finds `0x1000008C`, checks the descriptor is an
/// `Array`, indexes it by `windowID - 1` and reads the named property out of the `0x1000008B`
/// struct that lands there. A row with no `0x1000007F` is skipped rather than reported as 0 —
/// the chat-window option query answers `false` there and
/// leaves the text-type filter alone, which is the window's setup default and not "accept
/// nothing".
#[must_use]
pub fn decode_chat_filters(m: &PlayerModule) -> Vec<(u32, u64)> {
    let mut out = Vec::new();
    let Some(opts) = m.gameplay_options.as_ref() else {
        return out;
    };
    let Some(BasePropertyValue::Array(rows)) = opts.properties.get(placement::ARRAY) else {
        return out;
    };
    for (i, row) in rows.iter().enumerate() {
        let Some(BasePropertyValue::Struct(fields)) = row.value.as_ref() else {
            continue;
        };
        let Some(BasePropertyValue::Bitfield64(mask)) =
            fields.get(dereth_client_model::player::CHAT_TEXT_TYPE_FILTER)
        else {
            continue;
        };
        out.push((u32::try_from(i).unwrap_or(0) + 1, *mask));
    }
    out
}

fn placement_row(fields: &PropertyCollection) -> WindowPlacement {
    let int = |name: u32| match fields.get(name) {
        Some(BasePropertyValue::Integer(v)) => Some(*v),
        _ => None,
    };
    WindowPlacement {
        x: int(placement::X),
        y: int(placement::Y),
        w: int(placement::WIDTH),
        h: int(placement::HEIGHT),
        visible: match fields.get(placement::VISIBILITY) {
            Some(BasePropertyValue::Bool(v)) => Some(*v),
            _ => None,
        },
        title: match fields.get(placement::TITLE) {
            Some(BasePropertyValue::StringInfo(s)) if s.over == 1 => {
                Some(dereth_client_contract::ChatWindowTitle::Literal(
                    s.literal.clone().unwrap_or_default(),
                ))
            }
            Some(BasePropertyValue::StringInfo(s)) => {
                Some(dereth_client_contract::ChatWindowTitle::Table {
                    string_id: s.string_id,
                    table_id: s.table_id,
                })
            }
            _ => None,
        },
    }
}
