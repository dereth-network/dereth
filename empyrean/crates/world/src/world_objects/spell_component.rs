// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/SpellComponent.cs
//! Port of `Source/ACE.Server/WorldObjects/SpellComponent.cs`.

/// Non-property fields declared in `SpellComponent.cs`.
#[derive(Debug, Default)]
pub struct SpellComponentFields {}

// ACE: SpellComponent.SpellComponentDIDs
/// The DAT file containing DualDidMapper for spell component ids => wcids.
pub const SPELL_COMPONENT_DIDS: u32 = 0x2700_0002;

// ACE: SpellComponent.SpellComponentsTable
/// The spell components table from the portal.dat.
#[must_use]
pub fn spell_components_table(w: &crate::World) -> &empyrean_dat::file_types::SpellComponentsTable {
    crate::entity::spell_formula::spell_components_table(w)
}

// ACE: SpellComponent.BuildSpellComponentWCIDs
/// A lookup table of spell component wcids => component ids (`SpellComponentWCIDs`): each
/// component of the table mapped through the portal dat's `DualDidMapper`; a component the mapper
/// lacks is reported and skipped.
///
/// # Panics
/// When two components map to one wcid (ACE's `Dictionary.Add` throws).
// DIVERGE: ACE builds this once in the static constructor; the dats live in the World here, so it is built per call (the same table; the console line for a missing component repeats).
#[must_use]
pub fn build_spell_component_wcids(
    w: &crate::World,
) -> empyrean_common::dotnet::DotNetDict<u32, u32> {
    let mut spell_component_wcids = empyrean_common::dotnet::DotNetDict::new();

    let dual_dids = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::DualDidMapper>(SPELL_COMPONENT_DIDS);

    for &component_id in spell_components_table(w).components.keys() {
        // ClientEnumToID is the mapper's first table
        let wcid = dual_dids.as_ref().and_then(|m| {
            m.0.enum_to_id
                .iter()
                .find(|(k, _)| *k == component_id)
                .map(|(_, v)| *v)
        });
        let Some(wcid) = wcid else {
            empyrean_common::console_write_line!(
                "BuildSpellComponentWCIDs({component_id}): couldn't find component ID"
            );
            continue;
        };
        assert!(
            !spell_component_wcids.contains_key(&wcid),
            "ACE: ArgumentException: An item with the same key has already been added. Key: {wcid}"
        );
        spell_component_wcids.insert(wcid, component_id);
    }

    spell_component_wcids
}

// ACE: SpellComponent.IsValid
/// Returns TRUE if the input wcid is a valid spell component.
#[must_use]
pub fn is_valid(w: &crate::World, wcid: u32) -> bool {
    build_spell_component_wcids(w).contains_key(&wcid)
}

// ---- constructors and SetEphemeralValues ----

/// `new SpellComponent(weenie, guid)` / `new SpellComponent(biota)`: the `Stackable` constructor, then
/// SpellComponent's `SetEphemeralValues`.
// ACE: SpellComponent.SpellComponent
pub fn spell_component_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::stackable::stackable_ctor(o, env, src);
    spell_component_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: SpellComponent.SetEphemeralValues
fn spell_component_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
