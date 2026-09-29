// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Stackable.cs
//! Port of `Source/ACE.Server/WorldObjects/Stackable.cs`.

/// Non-property fields declared in `Stackable.cs`.
#[derive(Debug, Default)]
pub struct StackableFields {}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: Stackable.ActOnUse
#[allow(unused_variables)]
pub fn stackable_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    // Do nothing
}

// ---- constructors and SetEphemeralValues ----

/// `new Stackable(weenie, guid)` / `new Stackable(biota)`: the `WorldObject` constructor, then
/// Stackable's `SetEphemeralValues`.
// ACE: Stackable.Stackable
pub fn stackable_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    stackable_set_ephemeral_values(o, env);
}

// ACE: Stackable.SetEphemeralValues
fn stackable_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    if o.stack_size().is_none() {
        o.set_stack_size_prop(Some(1));
    }

    if o.max_stack_size().is_none() {
        o.set_max_stack_size(Some(1));
    }

    if o.value().is_none() {
        o.set_value(Some(0));
    }

    if o.encumbrance_val().is_none() {
        o.set_encumbrance_val(Some(0));
    }

    // Lifted int? arithmetic: null in, null out; `StackSize > 1` is false for null.
    if o.stack_unit_encumbrance().is_none() {
        if o.stack_size().is_some_and(|s| s > 1) {
            let v = o.encumbrance_val().zip(o.stack_size()).map(|(e, s)| e / s);
            o.set_stack_unit_encumbrance(v);
        } else {
            o.set_stack_unit_encumbrance(o.encumbrance_val());
        }
    }

    if o.stack_unit_value().is_none() {
        if o.stack_size().is_some_and(|s| s > 1) {
            let v = o.value().zip(o.stack_size()).map(|(e, s)| e / s);
            o.set_stack_unit_value(v);
        } else {
            o.set_stack_unit_value(o.value());
        }
    }

    // This is needed to fix stackables that were created before we removed the [Ephemeral] attribute from Encumbranceval and Value
    // In the distance future, this can be removed. 2019-02-13 Mag-nus
    if o.max_stack_size().is_some_and(|m| m > 1) {
        let stack_size = o.stack_size().unwrap_or(1);
        o.set_encumbrance_val(Some(
            o.stack_unit_encumbrance()
                .unwrap_or(0)
                .wrapping_mul(stack_size),
        ));
        o.set_value(Some(
            o.stack_unit_value().unwrap_or(0).wrapping_mul(stack_size),
        ));
    }
}

// ---- the stack helpers ----

impl crate::world_objects::world_object::WorldObject {
    /// In addition to setting StackSize, this will also set the EncumbranceVal and Value
    /// appropriately. Declared in `WorldObject_Properties.cs`; ported here with the other
    /// stack rules.
    // ACE: WorldObject.SetStackSize
    pub fn set_stack_size(&mut self, value: Option<i32>) {
        let is_stackable = self.is_stackable();
        if !is_stackable {
            return;
        }

        self.set_stack_size_prop(value);

        let stack_size = self.stack_size().unwrap_or(1);
        self.set_encumbrance_val(Some(
            self.stack_unit_encumbrance()
                .unwrap_or(0)
                .wrapping_mul(stack_size),
        ));
        self.set_value(Some(
            self.stack_unit_value()
                .unwrap_or(0)
                .wrapping_mul(stack_size),
        ));
    }
}
