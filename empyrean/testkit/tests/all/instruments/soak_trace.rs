//! ACE: Source/ACE.Server/Managers/WorldManager.cs::UpdateWorld
//! Explicitly requested virtual-time soak trace.

mod diagnostic {
    //! ACE: Source/ACE.Server/Managers/WorldManager.cs::UpdateWorld
    use crate::soak::soak_runs::*;

    /// A diagnostic (ignored; run by name): `SOAK_TRACE_ONLY` (fight, hunt, loot, wander, chat, relog, idle, trade;
    /// default fight) bots, `SOAK_TRACE_BOTS` of them (default 1), every one traced, for six virtual
    /// minutes.
    #[test]
    #[ignore = "instrument: writes a virtual-time soak trace; run explicitly by name"]
    fn soak_trace() {
        use soak::bot::Kind;
        let only = match std::env::var("SOAK_TRACE_ONLY").as_deref() {
            Ok("loot") => Kind::Loot,
            Ok("wander") => Kind::Wander,
            Ok("chat") => Kind::Chat,
            Ok("relog") => Kind::Relog,
            Ok("idle" | "trade") => Kind::Idle,
            Ok("hunt") => Kind::Fight,
            _ => Kind::Fight,
        };
        let bots: usize = std::env::var("SOAK_TRACE_BOTS")
            .ok()
            .and_then(|b| b.parse().ok())
            .unwrap_or(1);
        soak("trace", bots, 0.1, |c| {
            c.only = Some(only);
            match std::env::var("SOAK_TRACE_ONLY").as_deref() {
                Ok("trade") => c.trade_rate = 0.2,
                Ok("hunt") => c.hunter_every = 1,
                _ => c.hunter_every = 0,
            }
            c.trace = (0..bots).collect();
            c.sample_every = 60.0;
            c.tail = 600.0;
            c.decode_all = true;
        });
    }
}
