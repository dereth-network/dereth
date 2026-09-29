// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/FoodState.cs
//! Port of `Source/ACE.Server/Entity/FoodState.cs`.

use empyrean_entity::enums::{MotionCommand, MotionStance};
use empyrean_entity::ObjectGuid;

use crate::World;

/// The action to perform when the eat/drink motion has completed (`Action`).
pub type FoodCallback = Box<dyn FnOnce(&mut World) + Send>;

// ACE: FoodState
/// Fast chugging state variables. `Player.FoodState` (`Player_Use.cs`) holds one per player.
pub struct FoodState {
    /// A reference to the Player for this FoodState
    // ACE: FoodState.Player
    pub player: ObjectGuid,
    /// This is set to true when a FastTick player is consuming food / drink and allow_fast_chug
    /// = true
    // ACE: FoodState.IsChugging
    pub is_chugging: bool,
    /// The eat/drink motion to wait for AnimationDone
    // ACE: FoodState.UseMotion
    pub use_motion: MotionCommand,
    /// The action to perform when UseMotion has completed
    // ACE: FoodState.Callback
    pub callback: Option<FoodCallback>,
    /// Similar requirements as previous var
    // ACE: FoodState.UseAnimTime
    pub use_anim_time: f32,
    /// For returning to combat state after consuming
    // ACE: FoodState.PrevStance
    pub prev_stance: MotionStance,
}

impl std::fmt::Debug for FoodState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FoodState")
            .field("player", &self.player)
            .field("is_chugging", &self.is_chugging)
            .field("use_motion", &self.use_motion)
            .field("callback", &self.callback.as_ref().map(|_| "Action"))
            .field("use_anim_time", &self.use_anim_time)
            .field("prev_stance", &self.prev_stance)
            .finish()
    }
}

impl FoodState {
    // ACE: FoodState.FoodState
    /// `new FoodState(player)`: every other member at its C# default.
    #[must_use]
    pub fn new(player: ObjectGuid) -> Self {
        FoodState {
            player,
            is_chugging: false,
            use_motion: MotionCommand::default(),
            callback: None,
            use_anim_time: 0.0,
            prev_stance: MotionStance::default(),
        }
    }

    /// Called when a player starts performing the motion to apply a consumable
    // ACE: FoodState.StartChugging
    pub fn start_chugging(
        &mut self,
        use_motion: MotionCommand,
        callback: FoodCallback,
        use_anim_time: f32,
        prev_stance: MotionStance,
    ) {
        self.is_chugging = true;

        self.use_motion = use_motion;

        self.callback = Some(callback);

        self.use_anim_time = use_anim_time;

        self.prev_stance = prev_stance;
    }

    /// Called when a player completes the UseMotion
    // ACE: FoodState.FinishChugging
    pub fn finish_chugging(&mut self) {
        self.is_chugging = false;

        self.use_motion = MotionCommand::Invalid;

        self.callback = None;

        self.use_anim_time = 0.0;

        self.prev_stance = MotionStance::Invalid;
    }
}

impl Default for FoodState {
    /// A `FoodState` for no player (the `PlayerUseFields` default; `Player`'s constructor sets
    /// the real one).
    fn default() -> Self {
        FoodState::new(ObjectGuid::default())
    }
}
