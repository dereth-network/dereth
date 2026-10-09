//! Several passes share the world replay.

use dereth_render_hifi::graph::Chain;
use dereth_render_hifi::{DrawAction, DrawNote, ReplayFilter, SkipRule};

/// A filter that leaves out the draws at the commands it is given, and counts what it is asked.
struct Leaves {
    out: Vec<u32>,
    rule: SkipRule,
    asked: u32,
}

impl ReplayFilter for Leaves {
    fn draw(&mut self, cmd: u32, _note: Option<&DrawNote>) -> DrawAction<'_> {
        self.asked += 1;
        if self.out.contains(&cmd) {
            DrawAction::Skip(self.rule)
        } else {
            DrawAction::Legacy
        }
    }
}

fn rule(a: &DrawAction<'_>) -> Option<SkipRule> {
    match a {
        DrawAction::Skip(r) => Some(*r),
        _ => None,
    }
}

/// Behaviour: hifi.graph.several-passes-share-the-world-replay
#[test]
fn each_draw_goes_to_the_first_filter_that_acts_on_it() {
    let mut sky = Leaves {
        out: vec![1, 2],
        rule: SkipRule::SkyDome,
        asked: 0,
    };
    let mut ground = Leaves {
        out: vec![2, 3],
        rule: SkipRule::CoveredByFeed,
        asked: 0,
    };
    {
        let mut chain = Chain(vec![&mut sky, &mut ground]);
        assert_eq!(rule(&chain.draw(1, None)), Some(SkipRule::SkyDome));
        // Both would act on 2: the first in the chain decides.
        assert_eq!(rule(&chain.draw(2, None)), Some(SkipRule::SkyDome));
        assert_eq!(rule(&chain.draw(3, None)), Some(SkipRule::CoveredByFeed));
        assert!(matches!(chain.draw(4, None), DrawAction::Legacy));
    }
    // Every filter is asked about a draw no earlier one acted on.
    assert!(ground.asked >= 2);
    let mut empty = Chain(Vec::new());
    assert!(matches!(empty.draw(0, None), DrawAction::Legacy));
}
