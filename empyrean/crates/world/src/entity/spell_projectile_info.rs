// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/SpellProjectileInfo.cs
//! Port of `Source/ACE.Server/Entity/SpellProjectileInfo.cs`.
//!
//! The projectile, its caster and target are guids. `StartPos` is the
//! physics position (`Physics.Common.Position`, the shared crate's position). The members of the
//! `SpellProjectile` leaf class the diagnostic string reads (`Spell`, `Velocity`, `SpawnPos`) are
//! not ported yet; `ToString` reaches them through `not_ported!` pointers.

use dereth_primitives::Position as PPosition;
use empyrean_common::dotnet::{format, to_string};
use empyrean_entity::{ObjectGuid, Position, Vector3};

use crate::World;

// ACE: SpellProjectileInfo
#[derive(Debug, Clone)]
pub struct SpellProjectileInfo {
    // ACE: SpellProjectileInfo.SpellProjectile
    pub spell_projectile: ObjectGuid,
    // ACE: SpellProjectileInfo.CasterPos
    pub caster_pos: Option<Position>,
    // ACE: SpellProjectileInfo.TargetPos
    pub target_pos: Option<Position>,
    // ACE: SpellProjectileInfo.CachedVelocity
    pub cached_velocity: Option<Vector3>,
    // ACE: SpellProjectileInfo.StartPos
    pub start_pos: Option<PPosition>,
}

/// `obj.PhysicsObj.CachedVelocity` (ACE dereferences `PhysicsObj`; a body not in the physics
/// world reads as ACE's `NullReferenceException`). A target that left the world through its
/// landblock keeps its destroyed body in ACE, which still answers its last cached velocity
/// (`destroyed_physics_obj`).
fn cached_velocity(w: &World, obj: ObjectGuid) -> Vector3 {
    let o = w
        .objects
        .get(obj)
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    let v = match o.phys {
        Some(h) => {
            w.physics
                .get(h)
                .expect("ACE: PhysicsObj is null (NullReferenceException)")
                .cached_velocity
        }
        None => {
            o.wo.world_object
                .destroyed_physics_obj
                .expect("ACE: PhysicsObj is null (NullReferenceException)")
                .cached_velocity
        }
    };
    Vector3::new(v.x, v.y, v.z)
}

impl SpellProjectileInfo {
    /// # Panics
    /// When the projectile or its target has no physics object (ACE: `NullReferenceException`).
    // ACE: SpellProjectileInfo.SpellProjectileInfo
    #[must_use]
    pub fn new(w: &World, spell_projectile: ObjectGuid) -> Self {
        let links = w
            .objects
            .get(spell_projectile)
            .and_then(|o| o.projectile)
            .unwrap_or_default();
        let caster = links.source.filter(|&g| w.objects.get(g).is_some());
        let target = links.target.filter(|&g| w.objects.get(g).is_some());

        let mut info = SpellProjectileInfo {
            spell_projectile,
            caster_pos: None,
            target_pos: None,
            cached_velocity: None,
            start_pos: None,
        };

        if let Some(location) = caster
            .and_then(|c| w.objects.get(c))
            .and_then(|c| c.location())
        {
            info.caster_pos = Some(Position::from_position(&location));
        }

        if let Some(location) = target
            .and_then(|t| w.objects.get(t))
            .and_then(|t| t.location())
        {
            info.target_pos = Some(Position::from_position(&location));
        }

        let h = w
            .objects
            .get(spell_projectile)
            .and_then(|o| o.phys)
            .expect("ACE: SpellProjectile.PhysicsObj is null (NullReferenceException)");
        if let Some(pos) = crate::physics::phys_ext::position(w, h) {
            info.start_pos = Some(pos);
        }

        info.cached_velocity = target.map(|t| cached_velocity(w, t));

        info
    }

    /// The multi-line diagnostic dump.
    ///
    /// # Panics
    /// When the caster is gone (ACE: `NullReferenceException` on `caster.Name`).
    // ACE: SpellProjectileInfo.ToString
    #[must_use]
    pub fn to_string(&self, w: &World) -> String {
        let links = w
            .objects
            .get(self.spell_projectile)
            .and_then(|o| o.projectile)
            .unwrap_or_default();
        let caster = links
            .source
            .filter(|&g| w.objects.get(g).is_some())
            .expect("ACE: caster is null (NullReferenceException)");
        let target = links.target.filter(|&g| w.objects.get(g).is_some());
        let name = |g: ObjectGuid| crate::dispatch::name::name(w, g).unwrap_or_default();

        let mut info = format!("Caster: {} ({})\n", name(caster), caster);
        info += &format!(
            "CasterPos: {}\n",
            self.caster_pos
                .as_ref()
                .map(Position::to_loc_string)
                .unwrap_or_default()
        );
        let (spell_id, spell_name) = spell_projectile_spell(w, self.spell_projectile);
        info += &format!("Spell: {spell_id} - {spell_name}\n");
        info += &format!(
            "Velocity: {}\n",
            vector3_to_string(spell_projectile_velocity(w, self.spell_projectile))
        );
        info += &format!(
            "CachedVelocity: {}\n",
            self.cached_velocity
                .map(vector3_to_string)
                .unwrap_or_default()
        );
        info += &format!(
            "StartPos: {}\n",
            spell_projectile_spawn_pos(w, self.spell_projectile)
                .as_ref()
                .map(Position::to_loc_string)
                .unwrap_or_default()
        );
        info += &format!(
            "ActualStartPos: {}\n",
            self.start_pos
                .as_ref()
                .map(physics_position_to_string)
                .unwrap_or_default()
        );
        info += &format!(
            "EndPos: {}\n",
            w.objects
                .get(self.spell_projectile)
                .and_then(|o| o.location())
                .as_ref()
                .map(Position::to_loc_string)
                .unwrap_or_default()
        );

        if let Some(target) = target {
            let wcid = w.objects.get(target).map_or(0, |o| o.biota.weenie_class_id);
            info += &format!("Target: {} - {} ({})\n", wcid, name(target), target);
            info += &format!(
                "TargetPos: {}",
                self.target_pos
                    .as_ref()
                    .map(Position::to_loc_string)
                    .unwrap_or_default()
            );
        }

        info
    }
}

/// `Vector3.ToString()`: `<X, Y, Z>`, each in the "G" format (en-US).
#[must_use]
pub fn vector3_to_string(v: Vector3) -> String {
    format!(
        "<{}, {}, {}>",
        to_string(v.x),
        to_string(v.y),
        to_string(v.z)
    )
}

/// `Physics.Common.Position.ToString()`: `0x{ObjCellID:X8} {Frame}`, the frame as
/// `[x y z] w x y z`.
fn physics_position_to_string(p: &PPosition) -> String {
    let o = p.frame.origin;
    let r = p.frame.rotation;
    format!(
        "0x{} [{} {} {}] {} {} {} {}",
        format(p.cell.0, "X8"),
        to_string(o.x),
        to_string(o.y),
        to_string(o.z),
        to_string(r.w),
        to_string(r.x),
        to_string(r.y),
        to_string(r.z)
    )
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to `SpellProjectile` members.
// ---------------------------------------------------------------------------------------------

/// `SpellProjectile.Spell.Id` and `.Name` (ACE: `NullReferenceException` without a spell).
fn spell_projectile_spell(w: &World, sp: ObjectGuid) -> (u32, String) {
    let spell = crate::world_objects::spell_projectile::fields(w, sp)
        .spell
        .as_ref()
        .expect("ACE: Spell is null (NullReferenceException)");
    (spell.id(), spell.name().to_owned())
}

/// `SpellProjectile.Velocity` (the physics object's velocity).
fn spell_projectile_velocity(w: &World, sp: ObjectGuid) -> Vector3 {
    w.objects
        .get(sp)
        .and_then(|o| o.phys)
        .map_or(Vector3::ZERO, |h| crate::physics::phys_ext::velocity(w, h))
}

/// `SpellProjectile.SpawnPos`.
fn spell_projectile_spawn_pos(w: &World, sp: ObjectGuid) -> Option<Position> {
    crate::world_objects::spell_projectile::fields(w, sp).spawn_pos
}
