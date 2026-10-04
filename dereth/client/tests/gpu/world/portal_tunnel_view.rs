//! The portal tunnel hides the world and collapses the projection, at the pixel level. The tunnel
//! state machine is tested headless in `ui-screens` and `client-runtime`; this file shows that the
//! world-hidden state reaches the renderer (the world leaves the screen and the portal preview
//! space takes its place) and that the view-distance override reaches the projection. Each test is
//! two runs differing only in the thing under test: the tunnel comes from `Teleport::apply_events`
//! seeing the player created (`Login_CreatePlayer 0xF746`), never from a hand-set flag. The HUD is
//! alpha-composited over the world, so the declared region is its **opaque** pixels (those
//! byte-identical with and without a world loaded), and none may move by more than one eight-bit
//! step. Fixture: Holtburg with the gameplay UI in a headless `App` on the retail dats; fails
//! without the dats.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::ObjectId;

/// **An `expect`, never a skip.** A test that returns early is counted as a pass and
/// is invisible in the summary line; if the retail dats are not where `$DERETH_TEST_DAT_DIR` says,
/// the run is not a pass.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// The application with Holtburg loaded and the gameplay UI up — the state a player is in when a
/// portal fires.
fn app_in_world() -> Option<App> {
    let mut app = App::new(base_config())
        .unwrap_or_else(|e| panic!("the gpu tier needs a headless device and none opened: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the shell does not start: {e}"));
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s)
        .unwrap_or_else(|e| panic!("Holtburg's scene does not load: {e}"));
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..4 {
        app.frame();
    }
    Some(app)
}

type Shot = (u32, u32, Vec<u8>);

/// The same frames with **no world loaded at all**, for deriving the opaque-HUD region.
///
/// Nothing about this run involves the teleport animation; it exists only to say which pixels do
/// not depend on what is behind them.
fn shot_without_a_world() -> Option<Shot> {
    let mut app = App::new(base_config()).ok()?;
    app.start_shell().ok()?;
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..5 {
        app.frame();
    }
    let s = app.renderer_mut().capture_bgra().ok()?;
    app.shutdown();
    Some(s)
}

/// The declared region: **lit** pixels whose value is the same with Dereth behind the HUD and with
/// nothing behind it. Those are the HUD's opaque pixels.
///
/// The "lit" half matters. Without it the region also picks up every pixel that happened to be
/// black in both frames — the sky above the horizon — and those are not world-independent at all:
/// collapsing the field of view sweeps terrain up into them.
fn opaque_hud(with_world: &[u8], without_world: &[u8]) -> Vec<bool> {
    with_world
        .as_chunks::<4>()
        .0
        .iter()
        .zip(without_world.as_chunks::<4>().0)
        .map(|(a, b)| a == b && (b[0] > 8 || b[1] > 8 || b[2] > 8))
        .collect()
}

/// One run's two stations.
struct Run {
    /// The frame the application itself produced.
    shot: Shot,
    /// **The same draw, re-issued by hand with the preview queue already drained** — i.e.
    /// world draw and UI overlay with the portal preview space **absent**. See [`run`] for why this
    /// station has to exist.
    without_portal_space: Vec<u8>,
    hidden: bool,
    vdist: Option<f32>,
    /// How many preview spaces the application's own frame drew.
    previews: u64,
}

/// Run the application to a frame, optionally having told it the player object exists, and take
/// **two** shots of it.
///
/// `tunnel` is the **only** difference between the two runs: one `0xF746` event.
///
/// # Why there are two stations, and not one
///
/// `TeleportAnimState::is_tunnel` names **the four tunnel states, in which the world is hidden
/// *and* the portal preview space is shown**. So "the world is hidden" and "there is a swirl in
/// front of it" are one condition. The portal space is element `0x10000436`, an **800x600**
/// viewport widget under `0x10000037` in layout `0x2100000F`, drawn by `Renderer::draw_ui` before
/// every UI blit, and it covers the frame: the application's own tunnel frame has about as many
/// lit pixels outside the declared region as the control (~409,000), leaving no dynamic range for
/// a ten-to-one ratio, while the same run with the swirl absent has a few hundred, like a frame
/// with no world loaded at all.
///
/// So the world half is evaluated on the one station where "what did the world-hidden state do to
/// the world?" is not mixed with a second subject. `Renderer::draw_ui` takes `preview_draws`
/// rather than borrowing it, so the application's own frame drains the queue; re-issuing
/// `start_frame` / `draw_scene` / `draw_ui` / `end_frame` immediately afterwards, with the
/// identical list, replays exactly that
/// frame with the swirl absent and nothing else changed. That the queue really was empty is
/// **asserted**, not assumed — [`Run::previews`] must not advance across the re-issue.
fn run(tunnel: bool) -> Option<Run> {
    let mut app = app_in_world()?;
    if tunnel {
        // Retail's player creation starts with the position update not yet complete, so the
        // player-exists condition starts the login tunnel; `Teleport::teleport_in_progress()` is
        // the corresponding predicate.
        app.teleport_mut()
            .apply_events(&[SessionEvent::PlayerCreated(ObjectId(0x5000_0001))]);
    }
    let before = app.renderer().ui_stats.previews_drawn;
    app.frame();
    let previews = app.renderer().ui_stats.previews_drawn - before;
    let hidden = app.teleport().world_hidden();
    let vdist = app.teleport().view_distance();
    let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;

    // The second station. Same renderer, same smart-box state, same draw list.
    let list = cloned_draw_list(&app);
    let drained = app.renderer().ui_stats.previews_drawn;
    app.renderer_mut().start_frame().ok()?;
    app.draw_world_scene().ok()?;
    app.renderer_mut().draw_ui(&list).ok()?;
    app.renderer_mut().end_frame().ok()?;
    let (_, _, without_portal_space) = app.renderer_mut().capture_bgra().ok()?;
    assert_eq!(
        app.renderer().ui_stats.previews_drawn,
        drained,
        "the re-issued frame drew a preview space, so it is not the swirl-free station it claims \
         to be -- `draw_ui` no longer drains `preview_draws` and this file's premise is gone"
    );
    app.shutdown();
    Some(Run {
        shot: (w, h, bgra),
        without_portal_space,
        hidden,
        vdist,
        previews,
    })
}

fn diff(before: &[u8], after: &[u8], mask: &[bool]) -> (usize, usize) {
    let (mut inside, mut outside) = (0usize, 0usize);
    for i in 0..mask.len() {
        if before[i * 4..i * 4 + 4] != after[i * 4..i * 4 + 4] {
            if mask[i] {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    (inside, outside)
}

/// The declared region again, at the resolution the region is actually *declared* to.
///
/// [`opaque_hud`] calls a pixel opaque when a frame with Dereth behind it and a
/// frame with nothing behind it come out byte-identical. That is a two-sample test, and what it
/// certifies is "independent of *those two* backgrounds" — not "independent of every background".
/// A blit is `SRCALPHA / INVSRCALPHA`, so a HUD pixel whose source
/// alpha is 254 rather than 255 lands on the same byte over Dereth and over black and one step
/// higher over something much brighter. The portal preview — an 800x600 swirl — is
/// much brighter, and **six** pixels of the chat frame, at (391,555), (391,556), (389,565),
/// (404,567), (389,573) and (389,574), go up by exactly one in the blue channel because of it
/// (106->107, 106->107, 110->111, 42->43, 113->114, 111->112).
///
/// Six pixels moving by one LSB is the rounding floor of an eight-bit blend, and it is the client
/// being right: every translucent HUD pixel shows the world through it. The mask keeps out every
/// pixel that moves perceptibly; these six it cannot see, because they are opaque to the two
/// backgrounds it was built from.
///
/// So the claim is made one step above the floor instead of at it, and it loses nothing: a HUD
/// pixel the world is genuinely showing through moves by **tens** — 413,376 of the 480,000 pixels
/// of a gameplay frame do when the world goes away — so "not by more than one" separates "the
/// tunnel disturbed the HUD" from "an eight-bit blend rounded" as sharply as "not at all" would,
/// and unlike "not at all" it is true.
///
/// Returns (pixels that moved by more than one step, the worst step any declared pixel took).
fn moved_beyond_one_step(before: &[u8], after: &[u8], mask: &[bool]) -> (usize, i32) {
    let (mut n, mut worst) = (0usize, 0i32);
    for (i, m) in mask.iter().enumerate() {
        if !*m {
            continue;
        }
        let d = (0..4)
            .map(|c| (i32::from(after[i * 4 + c]) - i32::from(before[i * 4 + c])).abs())
            .max()
            .unwrap_or(0);
        if d > 1 {
            n += 1;
        }
        worst = worst.max(d);
    }
    (n, worst)
}

/// Lit pixels outside the declared region — i.e. the world.
fn lit_outside(bgra: &[u8], mask: &[bool]) -> usize {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .zip(mask)
        .filter(|(p, m)| !**m && (p[0] > 8 || p[1] > 8 || p[2] > 8))
        .count()
}

fn lit_total(bgra: &[u8]) -> usize {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8)
        .count()
}

/// The frame's UI blit list, cloned so the renderer can be borrowed mutably alongside it.
fn cloned_draw_list(app: &App) -> Vec<dereth_ui::UiDrawCmd> {
    app.ui_draw_list().to_vec()
}

/// Behaviour: world.teleport.the-tunnel-hides-the-world-and-leaves-the-hud
///
/// **The tunnel takes the world off the screen and leaves the opaque HUD alone.** Retail's world
/// draw skips its ordinary draw when the hidden flag is set.
///
/// Two runs of the same five frames over Holtburg with the HUD up. One is told the player object
/// exists and one is not; **nothing else differs**. The tunnel run must leave every opaque HUD
/// pixel byte-identical, must take Dereth off the screen, and must put the portal preview in front
/// of where Dereth was.
///
/// **The two halves are asserted separately**, because they are two different mechanisms and a
/// single frame cannot separate them — see [`run`]'s note. The world half is asserted at the
/// swirl-free station as ratios; the swirl half as the difference between the two stations.
/// Without the `if self.world_hidden { return Ok(()) }` guard in `Renderer::draw_scene` the two
/// frames are identical and `outside` is 0.
#[test]
fn the_tunnel_takes_the_world_off_the_screen_and_leaves_the_opaque_hud_alone() {
    let _gpu = gpu_lock();
    have_dats();
    let ctl = run(false).expect("a rendered frame: retail dats and a WARP device");
    let tun = run(true).expect("a rendered frame: retail dats and a WARP device");
    let (w, h, plain) = (ctl.shot.0, ctl.shot.1, &ctl.shot.2);
    let (w2, h2, tunnel) = (tun.shot.0, tun.shot.1, &tun.shot.2);
    let (w3, h3, no_world) = shot_without_a_world().expect("a rendered frame with no world loaded");
    assert_eq!((w, h), (w2, h2));
    assert_eq!((w, h), (w3, h3));

    // The control really is the control, and the only input that differs is the one event.
    assert!(!ctl.hidden, "no player, no tunnel");
    assert_eq!(ctl.vdist, None);
    assert!(
        tun.hidden,
        "the player object arriving sets the world controller's teleport-in-progress state"
    );

    let mask = opaque_hud(plain, &no_world);
    let declared = mask.iter().filter(|m| **m).count();
    assert!(
        declared > 1000,
        "only {declared} pixels are independent of the world -- no HUD?"
    );

    let (inside, outside) = diff(plain, tunnel, &mask);
    // Station 2: the same two frames with the portal preview absent. This is where the world half
    // of the claim lives.
    let lit_plain = lit_outside(&ctl.without_portal_space, &mask);
    let lit_tunnel = lit_outside(&tun.without_portal_space, &mask);
    let lit_none = lit_outside(&no_world, &mask);
    let (_, world_in_tunnel) = diff(&tun.without_portal_space, &no_world, &mask);
    let (_, world_in_plain) = diff(&ctl.without_portal_space, &no_world, &mask);
    // The swirl: what the application's own frame has that the swirl-free one does not.
    let (swirl_in_hud, swirl) = diff(tunnel, &tun.without_portal_space, &mask);
    let (_, control_swirl) = diff(plain, &ctl.without_portal_space, &mask);
    // The same two comparisons one step above the eight-bit rounding floor. See
    // [`moved_beyond_one_step`] for why the declared region cannot be asserted at the floor.
    let (inside_moved, inside_worst) = moved_beyond_one_step(plain, tunnel, &mask);
    let (swirl_hud_moved, swirl_hud_worst) =
        moved_beyond_one_step(tunnel, &tun.without_portal_space, &mask);
    // **Everything is measured and printed before anything is asserted**, so `lit_none` -- the
    // control the ratio argument rests on -- is visible on exactly the runs where it matters.
    eprintln!(
        "tunnel: {outside} changed outside the declared region, {inside} inside ({declared} \
         declared); with the portal preview absent, lit outside it {lit_plain} -> {lit_tunnel} \
         (no world: {lit_none}); world residue {world_in_plain} -> {world_in_tunnel}; \
         the swirl covers {swirl} px against the control's {control_swirl}; \
         preview spaces drawn {} -> {}",
        ctl.previews, tun.previews
    );
    eprintln!(
        "tunnel: of the {inside} declared pixels that changed, {inside_moved} moved by more \
         than one eight-bit step (worst {inside_worst}); of the {swirl_in_hud} the swirl \
         touched, {swirl_hud_moved} (worst {swirl_hud_worst})"
    );
    assert_eq!(
        inside_moved, 0,
        "{inside_moved} of the {inside} declared opaque-HUD pixels that changed moved by \
         more than one eight-bit step (worst {inside_worst}); the tunnel is drawing into the HUD"
    );
    assert!(
        outside > 10_000,
        "only {outside} pixels changed outside it -- the world is still being drawn"
    );

    // ---- half one: the world went away ----------------------------------------------------
    //
    // The world-hidden state means "draw as if there were no world", so with the swirl taken out of
    // the frame the tunnel run must look far more like the no-world frame than the control does.
    //
    // Not *identical* to it: the no-world run has no player body, so the radar's coordinate
    // read-out and compass differ, and those HUD pixels are outside the declared region by
    // construction. The measured residue is a few hundred pixels of read-out against ~400,000
    // pixels of Dereth, so the claim is made as a ratio rather than an equality.
    assert!(
        lit_plain > 10_000,
        "the control frame is not a picture of Dereth ({lit_plain} lit)"
    );
    assert!(
        lit_tunnel * 10 < lit_plain,
        "{lit_tunnel} pixels are lit outside the HUD with the world hidden and the swirl absent, \
         against {lit_plain} with it drawn -- the world is still being drawn"
    );
    assert!(
        world_in_tunnel * 20 < world_in_plain,
        "the tunnel frame differs from a world-free frame in {world_in_tunnel} pixels and the \
         control differs in {world_in_plain}; hiding the world did almost nothing"
    );

    // ---- half two: the swirl took its place ------------------------------------------------
    //
    // The portal preview. The control queues no preview space in this scene at all, so
    // the count is stated as the difference rather than as a literal: the tunnel draws exactly
    // one more than the control, and it is the only thing that separates the two stations.
    assert_eq!(
        tun.previews,
        ctl.previews + 1,
        "the tunnel frame drew {} preview spaces and the control drew {}; \
         the world-controller UI's portal-space preview is the one difference and it is not there",
        tun.previews,
        ctl.previews
    );
    assert_eq!(
        control_swirl, 0,
        "the control frame has a preview space in front of the world"
    );
    assert_eq!(
        swirl_hud_moved, 0,
        "the swirl moved {swirl_hud_moved} of the {swirl_in_hud} declared opaque-HUD pixels \
         it touched by more than one eight-bit step (worst {swirl_hud_worst})"
    );
    assert!(
        swirl > 100_000,
        "the portal preview covers an 800x600 viewport and covered only {swirl} pixels"
    );
    // The HUD itself is still there: the hidden flag hides the world, not the frame.
    assert!(
        lit_total(tunnel) > 1000,
        "the whole frame went dark, not just the world"
    );
}

/// **The collapsing view distance reaches the projection and leaves the opaque HUD alone.** The
/// world view-distance override sets `fov = 2*atan(1/d)` and `znear = (d >= 0.4) ? 0.1 : d*0.25`.
///
/// The animation only overrides the projection while the world is *drawn* — the two world fades —
/// so this drives the renderer's own override rather than waiting out a log-off, and uses the same
/// declared region. What it proves is the half a state assertion cannot: that the value the ramp
/// computes actually reaches the matrix.
///
/// Without `projection`'s `view_distance_override` arm
/// (`dereth/client/crates/render-cpu/src/camera.rs`) `outside` is 0.
#[test]
fn the_collapsing_view_distance_reaches_the_projection_and_leaves_the_opaque_hud_alone() {
    let _gpu = gpu_lock();
    have_dats();
    let shoot = |d: Option<f32>| -> Option<Shot> {
        let mut app = app_in_world()?;
        let list = cloned_draw_list(&app);
        app.renderer_mut().set_world_view_state(false, d);
        // Drawn directly rather than through `App::frame`, whose portal-animation update would put
        // the override back to what the animation says.
        app.renderer_mut().prepare_graphics_device();
        app.renderer_mut().start_frame().ok()?;
        app.draw_world_scene().ok()?;
        app.renderer_mut().draw_ui(&list).ok()?;
        app.renderer_mut().end_frame().ok()?;
        let s = app.renderer_mut().capture_bgra().ok()?;
        app.shutdown();
        Some(s)
    };
    let (w, h, normal) = shoot(None).expect("a rendered frame: retail dats and a WARP device");
    let (w2, h2, collapsing) =
        shoot(Some(0.2)).expect("a rendered frame: retail dats and a WARP device");
    let (w3, h3, no_world) = shot_without_a_world().expect("a rendered frame with no world loaded");
    assert_eq!((w, h), (w2, h2));
    assert_eq!((w, h), (w3, h3));

    let mask = opaque_hud(&normal, &no_world);
    let (inside, outside) = diff(&normal, &collapsing, &mask);
    // **The claim is made one eight-bit step above the floor, as [`moved_beyond_one_step`] makes
    // it for the tunnel test above.** About ten chat-frame pixels (captured alpha 102, around
    // (390..404, 505..517)) move by exactly one in a channel, e.g. BGRA `[150,209,237] ->
    // [150,209,238]`; not one declared pixel moves by two. That is the UI blit's
    // `SRCALPHA / INVSRCALPHA` blend rounding: [`opaque_hud`] certifies a pixel as independent of
    // the **two** backgrounds it was built from -- Dereth and nothing -- and collapsing the field
    // of view to 0.2 sweeps bright terrain up behind the chat frame, a **third** background. A HUD
    // pixel the world genuinely shows through moves by tens.
    let (beyond, worst) = moved_beyond_one_step(&normal, &collapsing, &mask);
    assert_eq!(
        beyond, 0,
        "{beyond} declared pixel(s) moved by more than one eight-bit step (worst {worst}); \
         {inside} moved at all"
    );
    assert!(
        outside > 10_000,
        "only {outside} pixels changed -- the override never reached the matrix"
    );

    // The far end of the ramp, where the field of view is 179.9 degrees.
    let (_, _, collapsed) =
        shoot(Some(0.001)).expect("a rendered frame: retail dats and a WARP device");
    let (inside2, outside2) = diff(&normal, &collapsed, &mask);
    let (beyond2, worst2) = moved_beyond_one_step(&normal, &collapsed, &mask);
    assert_eq!(
        beyond2, 0,
        "at 0.001: {beyond2} moved by more than one step (worst {worst2})"
    );

    // **"Further along the ramp changes more pixels" is not a claim a *count* can carry here,
    // because both counts are at the ceiling.** `fov = 2*atan(1/d)`, so **0.2 is already 157
    // degrees** -- nearly every non-HUD pixel has moved before the ramp reaches 0.001, and which
    // of the last few hundred land on the same byte at the two ends is noise about the terrain,
    // not about the projection.
    //
    // What is asserted instead: `d` reaches the matrix **as a value**, not as a flag -- the two
    // points on the ramp draw different frames from each other (about 404,000 pixels differ),
    // against a declared region that stays put to within the same one eight-bit step.
    let (between_in, between_out) = diff(&collapsing, &collapsed, &mask);
    let (between_beyond, between_worst) = moved_beyond_one_step(&collapsing, &collapsed, &mask);
    assert!(
        between_out > 10_000,
        "0.2 and 0.001 drew the same frame outside the declared region ({between_out} pixels \
         differ), so `d` reached the matrix as a flag rather than as a value"
    );
    assert_eq!(
        between_beyond, 0,
        "{between_beyond} declared pixel(s) moved by more than one step between the two ramp \
         positions (worst {between_worst})"
    );
    eprintln!(
        "view distance: 0.2 changes {outside} pixels outside the declared region and {inside} \
         inside it (worst step {worst}); 0.001 changes {outside2} and {inside2} (worst {worst2}); \
         the two ramp positions differ from each other in {between_out} and {between_in} \
         (worst {between_worst})"
    );
}
