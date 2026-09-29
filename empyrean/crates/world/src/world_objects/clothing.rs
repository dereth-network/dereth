// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Clothing.cs
//! Port of `Source/ACE.Server/WorldObjects/Clothing.cs`.

/// Non-property fields declared in `Clothing.cs`.
#[derive(Debug, Default)]
pub struct ClothingFields {}

/// This will also set the icon based on the palette: the icon of the palette in the clothing
/// table (0 for a missing table, ACE's empty `ClothingTable`), `PaletteTemplate` and `Shade`.
// ACE: Clothing.SetProperties
pub fn set_properties(
    w: &crate::World,
    o: &mut crate::world_objects::world_object::WorldObject,
    palette: i32,
    shade: f64,
) {
    use empyrean_common::dotnet::CsCast;
    use empyrean_dat::file_types::clothing_table::ClothingTableExt;

    let clothing_base = o.clothing_base().unwrap_or(0);
    let icon = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::ClothingTable>(clothing_base)
        .map_or(0, |t| t.get_icon(palette.cs_cast()));

    o.set_property(empyrean_entity::enums::PropertyDataId::Icon, icon);
    o.set_property(
        empyrean_entity::enums::PropertyInt::PaletteTemplate,
        palette,
    );
    o.set_property(empyrean_entity::enums::PropertyFloat::Shade, shade);
}

// ---- constructors and SetEphemeralValues ----

/// `new Clothing(weenie, guid)` / `new Clothing(biota)`: the `WorldObject` constructor, then
/// Clothing's `SetEphemeralValues`.
// ACE: Clothing.Clothing
pub fn clothing_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    clothing_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: Clothing.SetEphemeralValues
fn clothing_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
