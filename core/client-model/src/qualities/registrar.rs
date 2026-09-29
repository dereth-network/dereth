//! `QualityNotifications` — the change-notification fan-out.
//!
//! The registrar's change and remove fan-out.
//!
//! The client's registrar owns three handler sets and fires up to all three, **in this order**:
//!
//! 1. the per-object set (or the *global-object* set when the object's id is 0);
//! 2. the player set, when the object is the local player;
//! 3. the global set, always.
//!
//! This crate does not own the handlers themselves — the UI does, and the thirteen implementers
//! are all UI panels. What this crate owns is the *order*, because it decides which panel sees a
//! change first, so the registrar is modelled as a router that produces the scope sequence and
//! leaves the delivery to the caller.

use crate::qualities::StatKey;
use dereth_primitives::ObjectId;

/// Which of the three handler sets a notification is being delivered to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityScope {
    /// The handlers registered for this object id, or the global handlers when the id is 0.
    Object(ObjectId),
    /// The player handlers — "whatever object is currently the player".
    Player,
    /// The global handlers — "any object".
    Global,
}

/// One notification, as the registrar would deliver it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityNotice {
    pub scope: QualityScope,
    pub object: ObjectId,
    pub key: StatKey,
    /// The changed handler versus the removed handler.
    pub removed: bool,
}

/// The fan-out itself. Stateless: the registration tables live with the listeners.
#[derive(Debug, Clone, Copy, Default)]
pub struct QualityNotifications;

impl QualityNotifications {
    /// The registrar fan-out's scope sequence for one update.
    ///
    /// A player-object change invokes up to three handler sets; an id-0 change starts at the global
    /// set and therefore fires it twice, which is exactly what the client's `if/else` does.
    #[must_use]
    pub fn fan_out(
        object: ObjectId,
        player: Option<ObjectId>,
        key: StatKey,
        removed: bool,
    ) -> Vec<QualityNotice> {
        let mut out = Vec::with_capacity(3);
        let notice = |scope| QualityNotice {
            scope,
            object,
            key,
            removed,
        };
        // 1. id 0 -> the global set; otherwise the object's own set.
        out.push(notice(if object.0 == 0 {
            QualityScope::Global
        } else {
            QualityScope::Object(object)
        }));
        // 2. the player set.
        if player == Some(object) {
            out.push(notice(QualityScope::Player));
        }
        // 3. the global set, always.
        out.push(notice(QualityScope::Global));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qualities::StatType;

    fn key() -> StatKey {
        StatKey::new(StatType::Int, 12)
    }

    /// Oracle: §7's three-step dispatch, transcribed from the registrar's own
    /// change-handler call.
    #[test]
    fn a_player_change_fires_all_three_sets_in_order() {
        let p = ObjectId(0x5000_0001);
        let n = QualityNotifications::fan_out(p, Some(p), key(), false);
        assert_eq!(
            n.iter().map(|x| x.scope).collect::<Vec<_>>(),
            vec![
                QualityScope::Object(p),
                QualityScope::Player,
                QualityScope::Global
            ]
        );
    }

    #[test]
    fn a_non_player_change_skips_the_player_set() {
        let p = ObjectId(0x5000_0001);
        let other = ObjectId(0x8000_0002);
        let n = QualityNotifications::fan_out(other, Some(p), key(), false);
        assert_eq!(
            n.iter().map(|x| x.scope).collect::<Vec<_>>(),
            vec![QualityScope::Object(other), QualityScope::Global]
        );
    }

    /// The `if (obj->id == 0) use the global handler` branch is not exclusive with step 3, so the
    /// global set is genuinely fired twice. Preserved rather than deduplicated.
    #[test]
    fn an_id_zero_change_reaches_the_global_set_twice() {
        let n = QualityNotifications::fan_out(ObjectId(0), Some(ObjectId(1)), key(), true);
        assert_eq!(
            n.iter().map(|x| x.scope).collect::<Vec<_>>(),
            vec![QualityScope::Global, QualityScope::Global]
        );
        assert!(n.iter().all(|x| x.removed));
    }
}
