//! The client's records a world's rows make, from the install's own: what a world's overlay holds.
//!
//! A shop (`[[store]]`) is a copy of its store template, its saved state, under an id of the
//! world's own ([`crate::compile::Shop`]): the same stock, prices, sorter and portrait, with the
//! shop's title in place of the template's name, and its line under the title when it gives one.
//! The title is literal text, the same in every language. The client names a shop window from the
//! template its store view names, so a keeper of the shop has the copy named and sells the
//! template's own sales.

use std::collections::BTreeMap;

use auberean_assets::script::ScriptState;
use auberean_assets::Context;
use auberean_script::graph::{Body, Graph};
use auberean_script::library::Library;
use auberean_script::native_object::{NativeObject, TextInfo};

use crate::compile::Shop;

/// A record reader: a record's bytes by id, as the install holds it.
pub type Read<'a> = &'a dyn Fn(u32) -> Option<Vec<u8>>;

/// The portal records the world's shops make, by id: each shop's copy of its store template.
///
/// # Errors
/// A shop whose template does not read, whose copy's id the install holds, or whose template has
/// no title of the form a store's is.
pub fn shop_copies(
    shops: &[Shop],
    read: Read<'_>,
    ctx: &Context,
    lib: &Library,
) -> Result<BTreeMap<u32, Vec<u8>>, String> {
    let mut out = BTreeMap::new();
    for shop in shops {
        if read(shop.id).is_some() {
            return Err(format!(
                "store {}: the data files hold {:#010X} already; give the shop another id in world.lock",
                shop.key, shop.id
            ));
        }
        let bytes = read(shop.from)
            .ok_or_else(|| format!("store {}: no store template {:08X}", shop.key, shop.from))?;
        out.insert(
            shop.id,
            shop_copy(shop, &bytes, ctx, lib).map_err(|e| format!("store {}: {e}", shop.key))?,
        );
    }
    Ok(out)
}

/// One shop's copy of its store template, whose record is `bytes`.
///
/// # Errors
/// A record that is no saved state of the library's, or a template without a title.
pub fn shop_copy(
    shop: &Shop,
    bytes: &[u8],
    ctx: &Context,
    lib: &Library,
) -> Result<Vec<u8>, String> {
    let state: ScriptState = auberean_assets::decode(shop.from, bytes, ctx)
        .map_err(|e| format!("{:08X}: {e}", shop.from))?;
    let mut graph =
        Graph::decode(&state.graph, lib).map_err(|e| format!("{:08X}: {e}", shop.from))?;
    set_text(&mut graph, lib, "m_siName", &shop.name_text)?;
    if let Some(d) = &shop.description_text {
        set_text(&mut graph, lib, "m_siDesc", d)?;
    }
    let packed = graph.encode()?;
    Ok(auberean_assets::encode(
        shop.id,
        &ScriptState {
            graph: packed,
            package: state.package,
        },
    ))
}

/// Make the text the graph's root holds in `field` the literal `text`.
fn set_text(graph: &mut Graph, lib: &Library, field: &str, text: &str) -> Result<(), String> {
    let root = graph.root().ok_or("an empty saved state")?;
    let slot = {
        let Some(Body::Object(o)) = graph
            .ids
            .iter()
            .position(|id| *id == root)
            .map(|i| &graph.entries[i].body)
        else {
            return Err("a saved state whose root is no object".into());
        };
        let package = lib.package_name(o.package_id());
        let member = lib
            .frame(&package)
            .and_then(|f| f.members.iter().find(|m| m.name == field))
            .ok_or_else(|| format!("a {package} has no {field}"))?;
        usize::try_from(member.slot)
            .ok()
            .and_then(|s| o.slots.get(s).copied())
            .ok_or_else(|| format!("{field} is not in the saved state"))?
    };
    let at = graph
        .ids
        .iter()
        .position(|id| *id == slot)
        .ok_or_else(|| format!("{field} names nothing in the saved state"))?;
    match &mut graph.entries[at].body {
        Body::Object(o) => match &mut o.native {
            Some(NativeObject::TextInfo(t)) => {
                *t = TextInfo {
                    has_literal: 1,
                    literal: Some(text.encode_utf16().collect()),
                    ..TextInfo::default()
                };
                Ok(())
            }
            _ => Err(format!("{field} is not a text")),
        },
        _ => Err(format!("{field} is not a text")),
    }
}
