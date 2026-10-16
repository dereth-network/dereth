//! How a frame is re-shaded: which commands are re-shaded, how a step indoors is re-shaded, and
//! that every recorded draw of the world has an action.

use dereth_render::{PipelineKey, VertexFormat, ZFunc};
use dereth_render_hifi::derive::cache::{catalogue_keys, PipelineCache};
use dereth_render_hifi::derive::Variant;
use dereth_render_hifi::reshade::{
    classify, derived_key, readiness, translucent, DrawClass, IndoorPart, Layer, Layered,
    ReshadePlan,
};
use dereth_render_hifi::{
    Census, DrawAction, DrawNote, LegacyReplay, Mark, ReplayFilter, SideTables,
};

fn key(vertex_format: VertexFormat) -> PipelineKey {
    PipelineKey {
        vertex_format,
        ..catalogue_keys()[0]
    }
}

/// A world of `n` commands opened by the sky's first pass (commands 1 to 3), with a draw at
/// every command, its translucent draws flushed at `alpha`, and these extra marks.
fn tables(n: u32, alpha: u32, extra: &[(u32, Mark)]) -> SideTables {
    let mut t = SideTables::default();
    t.marks.push((1, Mark::WorldBegin));
    t.marks.push((1, Mark::SkyBegin(0)));
    t.marks.push((4, Mark::SkyEnd(0)));
    t.marks.extend_from_slice(extra);
    t.marks.push((alpha, Mark::AlphaFlush));
    t.marks.push((n, Mark::WorldEnd));
    t.marks.sort_by_key(|(at, _)| *at);
    let formats = VertexFormat::all();
    for cmd in 1..n {
        t.draws.push(DrawNote {
            cmd,
            key: key(formats[cmd as usize % formats.len()]),
            splat: cmd % 7 == 0,
            frame: 0,
            draw: 0,
            vertices: 3,
        });
    }
    t
}

/// Behaviour: hifi.reshade.a-frame-is-re-shaded-whole-and-a-step-indoors-re-shades-its-rooms
/// An outdoor frame is re-shaded from the end of the sky's first pass to the world's end, its
/// translucent draws after the rest, the sky and what comes before the world drawn as recorded. A
/// frame drawn over one depth -- underground, or in a room that shows nothing of the outdoors --
/// is re-shaded whole too, its rooms with the rest. A frame that steps from the outdoors into its
/// rooms by a depth clear re-shades both parts: the outdoors up to its flush before the clear,
/// the rooms from after the clear, each with its own translucent draws and its own flush of
/// held-back ones. Only a frame that clears the world's depth twice is re-shaded up to its first
/// step and replayed as recorded from there.
#[test]
fn an_outdoor_frame_and_an_indoor_one_are_re_shaded_whole_and_a_step_indoors_re_shades_its_rooms() {
    let outdoor = ReshadePlan::new(&tables(40, 30, &[])).expect("a world");
    assert_eq!(outdoor.pre, 0..1);
    assert_eq!(outdoor.sky, 1..4);
    assert_eq!(outdoor.recorded_before(), 0..4);
    assert_eq!(outdoor.opaque, 4..30);
    assert_eq!(outdoor.alpha, 30..40);
    assert!(outdoor.rest.is_empty() && !outdoor.is_split());
    assert_eq!((outdoor.step.clone(), outdoor.indoors_from), (None, 40));
    assert_eq!(outdoor.reshaded(), 4..40);

    // A world that does not open with the sky re-shades its sky with the rest.
    let mut skyless = tables(40, 30, &[]);
    skyless
        .marks
        .retain(|(_, m)| !matches!(m, Mark::SkyBegin(0) | Mark::SkyEnd(0)));
    skyless.marks.push((5, Mark::SkyBegin(0)));
    skyless.marks.push((8, Mark::SkyEnd(0)));
    skyless.marks.sort_by_key(|(at, _)| *at);
    let skyless = ReshadePlan::new(&skyless).expect("a world");
    assert_eq!((skyless.sky, skyless.opaque), (1..1, 1..30));

    // Underground: the rooms from the world's start, over one depth.
    let mut underground = tables(40, 30, &[(1, Mark::EnvCell { cell: 0x0101_0100 })]);
    underground
        .marks
        .retain(|(_, m)| !matches!(m, Mark::SkyBegin(0) | Mark::SkyEnd(0)));
    let underground = ReshadePlan::new(&underground).expect("a world");
    assert_eq!(
        (underground.opaque.clone(), underground.alpha.clone()),
        (1..30, 30..40)
    );
    assert!(underground.rest.is_empty() && underground.step.is_none());
    assert_eq!(underground.indoors_from, 1);
    assert_eq!(underground.reshaded(), 1..40);

    // An indoor flush, or an indoor cell drawn outside the rooms of buildings seen from
    // outdoors, with no depth clear after it is drawn over the same depth: no step.
    for extra in [
        vec![(12, Mark::IndoorFlush)],
        vec![
            (9, Mark::EnvCell { cell: 0x0101_0100 }),
            (15, Mark::IndoorFlush),
        ],
    ] {
        let plan = ReshadePlan::new(&tables(40, 30, &extra)).expect("a world");
        assert_eq!((plan.opaque, plan.alpha), (4..30, 30..40), "{extra:?}");
        assert!(plan.step.is_none() && plan.rest.is_empty(), "{extra:?}");
        assert_eq!(plan.indoors_from, extra[0].0);
    }

    // Rooms of buildings seen from outdoors are drawn into the outdoor world, not a step indoors.
    let rooms = ReshadePlan::new(&tables(
        40,
        30,
        &[
            (6, Mark::Interiors),
            (7, Mark::EnvCell { cell: 0x0101_0100 }),
            (11, Mark::InteriorsEnd),
            (14, Mark::Interiors),
            (14, Mark::InteriorsEnd),
            (20, Mark::Interiors),
            (21, Mark::EnvCell { cell: 0x0101_0101 }),
            (25, Mark::InteriorsEnd),
        ],
    ))
    .expect("a world");
    assert!(!rooms.is_split() && rooms.step.is_none(), "{rooms:?}");
    assert_eq!(rooms.interiors, [6..11, 20..25]);

    // A room that shows the outdoors: the outdoors, its flush before the clear, the clear, then
    // the stamps and the rooms up to their own flush.
    let mut doorway = tables(
        40,
        30,
        &[
            (6, Mark::Interiors),
            (8, Mark::InteriorsEnd),
            (12, Mark::IndoorFlush),
            (18, Mark::EnvCell { cell: 0x0101_0100 }),
            (22, Mark::Interiors),
            (24, Mark::InteriorsEnd),
        ],
    );
    doorway.clears.push(15);
    let doorway = ReshadePlan::new(&doorway).expect("a world");
    assert_eq!(
        (doorway.opaque.clone(), doorway.alpha.clone()),
        (4..12, 12..15)
    );
    assert_eq!(
        doorway.step,
        Some(IndoorPart {
            clear: 15,
            opaque: 16..30,
            translucent: 30..30,
            alpha: 30..40,
        })
    );
    assert!(doorway.steps_indoors() && !doorway.is_split());
    assert_eq!((doorway.indoors_from, doorway.reshaded()), (12, 4..40));
    // Only the rooms of buildings drawn before the step are the outdoors'.
    assert_eq!(
        (doorway.interiors.len(), doorway.interiors.first()),
        (1, Some(&(6..8)))
    );

    // A clear after the flush of the held-back draws: the outdoors keeps its flush, and the rooms
    // after the clear have none of their own.
    let mut cleared = tables(40, 30, &[]);
    cleared.clears.push(0); // a clear before the world is not the world's
    cleared.clears.push(33);
    let cleared = ReshadePlan::new(&cleared).expect("a world");
    assert_eq!(
        (cleared.opaque.clone(), cleared.alpha.clone()),
        (4..30, 30..33)
    );
    assert!(cleared.rest.is_empty());
    assert_eq!(
        cleared.step.map(|s| (s.clear, s.opaque, s.alpha)),
        Some((33, 34..40, 40..40))
    );

    // Two clears of the world: re-shaded up to the first step, drawn as recorded after it.
    let mut twice = tables(40, 30, &[(12, Mark::IndoorFlush)]);
    twice.clears.extend([15, 25]);
    let twice = ReshadePlan::new(&twice).expect("a world");
    assert_eq!(
        (
            twice.opaque.clone(),
            twice.alpha.clone(),
            twice.rest.clone()
        ),
        (4..12, 12..12, 12..40)
    );
    assert!(twice.is_split() && twice.step.is_none());

    let mut no_world = SideTables::default();
    no_world.marks.push((0, Mark::WorldBegin));
    assert_eq!(
        ReshadePlan::new(&no_world),
        None,
        "a world that never ended"
    );
}

/// Behaviour: hifi.reshade.a-frame-is-re-shaded-whole-and-a-step-indoors-re-shades-its-rooms
/// In a room that shows the outdoors, the rooms' own translucent draws before their flush are
/// held back to be drawn after the work over the opaque world, as the outdoors' are.
#[test]
fn the_rooms_translucent_draws_after_a_step_indoors_are_held_back_as_the_outdoors_are() {
    let opaque = catalogue_keys()[0];
    let smoke = PipelineKey {
        alpha_blend: true,
        alpha_test: false,
        z_write: false,
        z_func: ZFunc::Less,
        ..opaque
    };
    let mut t = tables(40, 30, &[(12, Mark::IndoorFlush)]);
    t.clears.push(15);
    for d in &mut t.draws {
        d.splat = false;
        d.key = if matches!(d.cmd, 7 | 21 | 26) {
            smoke
        } else {
            opaque
        };
    }
    let plan = ReshadePlan::new(&t).expect("a world");
    assert_eq!(plan.translucent, 7..12);
    let step = plan.step.expect("a step indoors");
    assert_eq!((step.opaque, step.translucent), (16..30, 21..30));
}

/// Behaviour: hifi.reshade.every-world-draw-has-an-action
/// Every noted draw of the world is classified for the re-shade: waiting for its pipeline until it
/// is built, whatever its vertex format and whether or not it is a landscape splat; only a draw
/// with no note is unclassified, and one such draw keeps the frame from being re-shaded.
#[test]
fn every_noted_world_draw_is_classified_and_an_unnoted_one_holds_the_frame_back() {
    let t = tables(60, 50, &[(20, Mark::Objects), (40, Mark::Particles)]);
    let plan = ReshadePlan::new(&t).expect("a world");
    let cache = PipelineCache::new();
    let mut splats = 0;
    for note in t.draws.iter().filter(|d| plan.reshaded().contains(&d.cmd)) {
        let class = classify(Some(note), &cache);
        assert_ne!(class, DrawClass::Unclassified, "{note:?}");
        assert_eq!(class, DrawClass::Waiting(derived_key(note)));
        let k = derived_key(note);
        assert_eq!(
            (k.key, k.splat, k.variant),
            (note.key, note.splat, Variant::Reshade)
        );
        splats += usize::from(note.splat);
    }
    assert!(splats > 0);
    assert_eq!(classify(None, &cache), DrawClass::Unclassified);
    let r = readiness(&plan, &t, &cache);
    // Commands 4 to 59: everything after the sky.
    assert_eq!(r.waiting, 56);
    assert_eq!((r.ready, r.failed, r.unclassified), (0, 0, 0));
    assert!(!r.all_ready());

    let mut unnoted = t.clone();
    unnoted.unnoted.push(21);
    let r = readiness(&plan, &unnoted, &cache);
    assert_eq!(r.unclassified, 1);
}

/// Behaviour: hifi.reshade.every-world-draw-has-an-action
/// The pipelines asked for when re-shading starts include the whole catalogue the device itself
/// can build, in every vertex format, and asking twice builds once.
#[test]
fn the_catalogue_keys_cover_every_vertex_format_and_asking_twice_asks_once() {
    let keys = catalogue_keys();
    assert_eq!(
        keys.len(),
        dereth_render_cpu::pso::CATALOGUE.len() * VertexFormat::all().len()
    );
    for f in VertexFormat::all() {
        assert!(keys.iter().any(|k| k.vertex_format == f));
    }
    let mut cache = PipelineCache::new();
    let k = derived_key(&DrawNote {
        cmd: 3,
        key: keys[0],
        splat: false,
        frame: 0,
        draw: 0,
        vertices: 3,
    });
    cache.ask(k);
    cache.ask(k);
    assert_eq!(cache.waiting(), 1);
    assert!(!cache.is_ready(&k) && !cache.has_failed(&k));
}

/// A draw at `cmd` drawn with `key`.
fn note(cmd: u32, key: PipelineKey) -> DrawNote {
    DrawNote {
        cmd,
        key,
        splat: false,
        frame: 0,
        draw: 0,
        vertices: 3,
    }
}

/// Behaviour: hifi.reshade.a-frame-is-re-shaded-whole-and-a-step-indoors-re-shades-its-rooms
/// The translucent draws the world draws before its flush of held-back draws (smoke, glass, the
/// blended parts of objects) are left out of the replay of the opaque world and drawn by a second
/// replay of it after the work over the opaque world, in their recorded order; a draw that blends
/// but is cut out against its texture, writes depth, or passes the depth test whatever is there
/// stays where it was drawn, and every draw is counted once across the two replays.
#[test]
fn translucent_draws_of_the_opaque_world_are_drawn_after_it_and_counted_once() {
    let opaque = catalogue_keys()[0];
    let smoke = PipelineKey {
        alpha_blend: true,
        alpha_test: false,
        z_write: false,
        z_func: ZFunc::Less,
        ..opaque
    };
    let leaves = PipelineKey {
        alpha_test: true,
        z_write: true,
        ..smoke
    };
    let detail = PipelineKey {
        z_write: true,
        z_func: ZFunc::LessEqual,
        ..smoke
    };
    let sky_layer = PipelineKey {
        z_func: ZFunc::Always,
        ..smoke
    };
    assert!(translucent(&note(0, smoke)));
    for key in [opaque, leaves, detail, sky_layer] {
        assert!(!translucent(&note(0, key)), "{key:?}");
    }
    assert!(!translucent(&DrawNote {
        splat: true,
        ..note(0, smoke)
    }));

    // Smoke at 12 and 20 among the opaque world's draws, and one held back past the flush at 30.
    let mut t = tables(40, 30, &[]);
    for d in &mut t.draws {
        d.splat = false;
        d.key = match d.cmd {
            12 | 20 | 33 => smoke,
            8 => leaves,
            9 => detail,
            10 => sky_layer,
            _ => opaque,
        };
    }
    let plan = ReshadePlan::new(&t).expect("a world");
    assert_eq!(plan.opaque, 4..30);
    assert_eq!(plan.translucent, 12..30);
    assert_eq!(plan.alpha, 30..40);

    let drawn = |layer: Layer, range: std::ops::Range<u32>, census: &mut Census| {
        let mut inner = LegacyReplay;
        let mut f = Layered::new(layer, &mut inner);
        let mut out = Vec::new();
        for cmd in range {
            let action = f.draw(cmd, t.draw_note(cmd));
            census.count(&action);
            if matches!(action, DrawAction::Legacy) {
                out.push(cmd);
            }
        }
        out
    };
    let mut census = Census::default();
    let first = drawn(Layer::Opaque, plan.opaque.clone(), &mut census);
    let second = drawn(Layer::Translucent, plan.translucent.clone(), &mut census);
    assert!(!first.contains(&12) && !first.contains(&20));
    assert!(first.contains(&8) && first.contains(&9) && first.contains(&10));
    assert_eq!(second, [12, 20]);
    assert_eq!(census.total() as usize, plan.opaque.len());
    assert_eq!(census.legacy as usize, plan.opaque.len());

    // An opaque world with no translucent draw has nothing to draw after it.
    let none = ReshadePlan::new(&tables(40, 30, &[])).expect("a world");
    assert!(none.translucent.is_empty());
}
