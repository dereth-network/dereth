// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/RecordCast.cs
//! Port of `Source/ACE.Server/Entity/RecordCast.cs`: the `/recordcast` developer log of a
//! player's casting, buffered and appended to `{Name}Cast.log` at each cast's end.

use empyrean_entity::enums::CombatMode;
use empyrean_entity::ObjectGuid;

use crate::entity::spell::Spell;
use crate::network::motion::move_to_state::MoveToState;
use crate::network::structure::jump_pack::JumpPack;
use crate::World;

// ACE: RecordCast
/// `Player.RecordCast`. The `Player` link is the `player` argument of the members.
#[derive(Debug, Default, Clone)]
pub struct RecordCast {
    // ACE: RecordCast.Enabled
    pub enabled: bool,
    // ACE: RecordCast.Buffer
    pub buffer: String,
}

impl RecordCast {
    // ACE: RecordCast.RecordCast
    /// `new RecordCast(player)`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

fn record_cast(w: &mut World, player: ObjectGuid) -> &mut RecordCast {
    &mut w
        .objects
        .get_mut(player)
        .and_then(|o| o.player.as_mut())
        .expect("ACE: RecordCast.Player is a Player")
        .player_magic
        .record_cast
}

fn name(w: &World, player: ObjectGuid) -> String {
    crate::dispatch::name::name(w, player).unwrap_or_default()
}

// ACE: RecordCast.Filename
/// `$"{Player.Name}Cast.log"`.
#[must_use]
pub fn filename(w: &World, player: ObjectGuid) -> String {
    format!("{}Cast.log", name(w, player))
}

// ACE: RecordCast.OnMoveToState
pub fn on_move_to_state(w: &mut World, player: ObjectGuid, move_to_state: &MoveToState) {
    let raw_state = &move_to_state.raw_motion_state;

    let line = raw_state.to_string_flags(false).replace('\n', " | ");
    // `line.Substring(0, line.Length - 8)`, in UTF-16 units
    let units: Vec<u16> = line.encode_utf16().collect();
    let line = if units.len() >= 8 {
        String::from_utf16_lossy(&units[..units.len() - 8])
    } else {
        String::new()
    };

    output(w, player, &line);
}

// ACE: RecordCast.OnCastTargetedSpell
pub fn on_cast_targeted_spell(
    w: &mut World,
    player: ObjectGuid,
    spell: &Spell,
    target: ObjectGuid,
) {
    let line = format!(
        "HandleActionCastTargetedSpell({} - {}, {} ({target}))",
        spell.id(),
        spell.name(),
        name(w, target)
    );

    output(w, player, &line);
}

// ACE: RecordCast.OnCastUntargetedSpell
pub fn on_cast_untargeted_spell(w: &mut World, player: ObjectGuid, spell: &Spell) {
    let line = format!(
        "HandleActionCastUntargetedSpell({} - {})",
        spell.id(),
        spell.name()
    );

    output(w, player, &line);
}

// ACE: RecordCast.OnJump
/// `$"HandleActionJump: Velocity={jump.Velocity}, Extent={jump.Extent}"` (`Vector3.ToString()` is
/// `<x, y, z>`).
pub fn on_jump(w: &mut World, player: ObjectGuid, jump: &JumpPack) {
    let v = jump.velocity;
    let line = format!(
        "HandleActionJump: Velocity=<{}, {}, {}>, Extent={}",
        empyrean_common::dotnet::to_string(v.x),
        empyrean_common::dotnet::to_string(v.y),
        empyrean_common::dotnet::to_string(v.z),
        empyrean_common::dotnet::to_string(jump.extent)
    );

    output(w, player, &line);
}

// ACE: RecordCast.OnSetCombatMode
pub fn on_set_combat_mode(w: &mut World, player: ObjectGuid, combat_mode: CombatMode) {
    let line = format!(
        "HandleActionChangeCombatMode({})",
        combat_mode.to_dotnet_string()
    );

    output(w, player, &line);
}

// ACE: RecordCast.Log
pub fn log(w: &mut World, player: ObjectGuid, line: &str) {
    output(w, player, line);
}

// ACE: RecordCast.Output
/// Appends `[timestamp] line` to the buffer and writes it to the console.
// DIVERGE: ACE stamps DateTime.Now (local time); the port stamps the tick's UTC (injected clocks, V174).
pub fn output(w: &mut World, player: ObjectGuid, line: &str) {
    let timestamp = w.now.utc.format("yyyy-MM-dd hh:mm:ss,fff");

    let timestamp_line = format!("[{timestamp}] {line}");

    let rc = record_cast(w, player);
    rc.buffer.push_str(&timestamp_line);
    rc.buffer.push('\n');
    empyrean_common::console_write_line!("{timestamp_line}");
}

// ACE: RecordCast.Flush
/// `File.AppendAllText(Filename, Buffer.ToString())`, then clears the buffer.
///
/// # Panics
/// When the file cannot be written (ACE's `IOException`).
pub fn flush(w: &mut World, player: ObjectGuid) {
    use std::io::Write as _;

    let filename = filename(w, player);
    let text = std::mem::take(&mut record_cast(w, player).buffer);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&filename)
        .unwrap_or_else(|e| panic!("ACE: File.AppendAllText({filename}): {e}"));
    file.write_all(text.as_bytes())
        .unwrap_or_else(|e| panic!("ACE: File.AppendAllText({filename}): {e}"));
}
