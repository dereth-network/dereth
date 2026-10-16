//! The drawn ground's knobs: every value its look is made of, each named, with a default, a range
//! and what it changes, read from and written to a plain text form that a running client can
//! reload. The same file holds the knobs of the light an art pack's art is drawn with.
//!
//! The text form is a flat list of `name = value` lines, numbers or `true` and `false`. Lines
//! starting with `#` and `[section]` headers are only for reading: the names are unique without
//! them. A name the table does not know, or a value it cannot read, is skipped with a note; a
//! value outside its range is brought to the nearest end of it, with a note.

use std::fmt::Write as _;

/// One knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum Knob {
    /// The drawn ground at all.
    Ground,
    /// How densely the drawn ground is meshed.
    Detail,
    /// The share of the rounding kept on walkable ground.
    Round,
    /// The most the walkable ground leaves retail's surface.
    WalkCap,
    /// The width the walkable offset is smoothed over.
    WalkLowpass,
    /// The fine relief's height.
    Micro,
    /// The fine relief's size.
    MicroScale,
    /// How far the offset takes to rise from a pin.
    PinFade,
    /// The margin round a building's footprint.
    PinBuildingMargin,
    /// The margin round a static object's footprint.
    PinStaticMargin,
    /// Roads pinned.
    PinRoads,
    /// Water pinned.
    PinWater,
    /// How much water a point holds to be pinned.
    PinWaterLevel,
    /// Rock bands on unwalkable ground.
    Cliffs,
    /// The most a rock band leaves retail's surface.
    CliffCap,
    /// The same on tall faces.
    CliffCapTall,
    /// The relief a face has to be tall.
    CliffTallRelief,
    /// The width of unwalkable ground kept at retail's surface beside walkable ground.
    CliffEdge,
    /// How fast the rock bands' room grows beyond that edge.
    CliffGrowth,
    /// A band's height, as a multiple of the cap.
    CliffBand,
    /// The share of a band that is its riser.
    CliffRiser,
    /// How much the bands' edges are broken.
    CliffNoise,
    /// The size of that breaking.
    CliffNoiseScale,
    /// Bodies, items and the camera drawn on the drawn ground.
    BodyShift,
    /// The height over retail's ground the shift starts to fade at.
    ShiftFade,
    /// The height it is gone at.
    ShiftGone,
    /// The least room the camera keeps over the drawn ground.
    CameraClearance,
    /// The fine relief and the terraces leveled under bodies.
    Level,
    /// The radius of the level ground under a body.
    LevelRadius,
    /// The width it fades back over.
    LevelFade,
    /// Small terraces on steep walkable ground.
    Terraces,
    /// A terrace's height.
    TerraceBand,
    /// The most a terrace leaves retail's surface.
    TerraceCap,
    /// The slope terraces start at.
    TerraceFrom,
    /// How far, in blocks, the near offset is worked out round the viewer.
    NearBlocks,
    /// Where the near ground starts to give way to the far.
    MorphStart,
    /// Where it has.
    MorphEnd,
    /// Ground drawn to the horizon.
    Far,
    /// The share of the rounding far off.
    FarRound,
    /// The far ground's relief.
    FarDetail,
    /// The size of that relief.
    FarDetailScale,
    /// How far the ground is drawn.
    Horizon,
    /// How thick the air is over the far ground.
    Haze,
    /// Trees drawn far off, past the blocks round the viewer.
    FarTrees,
    /// How far the far trees reach.
    TreeReach,
    /// How far every far tree is drawn before they thin out.
    TreeThinFrom,
    /// How much wider the trees left standing in a thinned forest stand.
    TreeWiden,
    /// How much softer far trees are drawn than their size on screen calls for.
    TreeSoften,
    /// How strongly forest cover tints the far ground.
    ForestCover,
    /// How high forest cover raises the far ground.
    ForestCanopy,
    /// Scree and rock laid by the drawn slope.
    Slope,
    /// The slope scree starts at.
    ScreeFrom,
    /// The slopes scree blends in over.
    ScreeBlend,
    /// The slope rock starts at.
    RockFrom,
    /// The slopes rock blends in over.
    RockBlend,
    /// How much of the scree and rock is laid.
    SlopeStrength,
    /// How much the slope scree and rock are laid by is the smooth surface's.
    SlopeSmooth,
    /// How much scree and rock show through snow and ice.
    SlopeOnSnow,
    /// The terrain type scree is drawn with.
    ScreeGround,
    /// The terrain type rock is drawn with.
    RockGround,
    /// The water drawn as water.
    Water,
    /// The water's own color.
    WaterColor,
    /// How far the eye sees into the water.
    WaterClarity,
    /// The deepest the bed lies under the water.
    WaterDepth,
    /// How far from the shore the bed reaches that depth.
    WaterShelf,
    /// The ripples' size.
    RippleScale,
    /// How fast the ripples move.
    RippleSpeed,
    /// How much the ripples bend the surface.
    RippleStrength,
    /// How far from the eye the ripples calm.
    RippleCalm,
    /// How strongly the water rings round a body standing in it.
    WaterRings,
    /// How strongly the water reflects.
    WaterReflection,
    /// How far the water takes to come in at its edge.
    ShoreSoft,
    /// How much darker the wet bank is.
    ShoreWet,
    /// The terrain type the bed is drawn with away from the shore.
    WaterBed,
    /// How much grass grows in over a muddy bank.
    BankGreen,
    /// The terrain type grown in over a muddy bank where no other lies beside it.
    BankGrass,
    /// A diagnostic view.
    View,
    /// How much an art pack's lit surfaces count in the outdoor exposure.
    PackExposure,
    /// How much an art pack's leaves count there.
    LeafExposure,
    /// How much an art pack's ground counts there.
    GroundExposure,
    /// How much of the glow of the retail object's own surfaces an art pack's model keeps.
    PackGlow,
    /// The open sky a leaf deep in an art pack's crown sees.
    LeafInnerSky,
    /// The sun a leaf on the outside of an art pack's crown keeps in shade.
    LeafScatter,
    /// How much of that a leaf deep inside takes.
    LeafScatterDeep,
    /// The Remastered sky at all.
    Sky,
    /// The Remastered clouds.
    Clouds,
    /// How many steps the clouds are drawn with through their depth.
    CloudSteps,
    /// The share of the frame's pixels each way the clouds are worked out at.
    CloudResolution,
    /// How much of the sky the clouds cover, against retail's own picture.
    CloudCover,
    /// How high over the eye the clouds' base lies.
    CloudHeight,
    /// How thick the clouds are.
    CloudDepth,
    /// How large retail's cloud picture is laid over the sky.
    CloudScale,
    /// How much fine detail breaks up the clouds.
    CloudDetail,
    /// How fast the clouds drift, a share of retail's drift.
    CloudSpeed,
    /// How brightly the sun lights the clouds.
    CloudLight,
    /// How dark the clouds' shaded sides and bases are.
    CloudShade,
    /// How much the clouds shade the land under them.
    CloudShadows,
    /// The sun's disk across.
    SunSize,
    /// How strong the sun's glow is.
    SunGlow,
    /// The moons' size against retail's.
    MoonSize,
    /// How bright the stars are.
    Stars,
    /// How much the stars twinkle.
    StarTwinkle,
}

/// What a knob is and holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KnobSpec {
    /// The knob.
    pub knob: Knob,
    /// Its name in the text form.
    pub name: &'static str,
    /// Its default.
    pub default: f32,
    /// The least value it takes.
    pub min: f32,
    /// The greatest.
    pub max: f32,
    /// Whether it is a switch: 0 off, 1 on, written `false` and `true`.
    pub switch: bool,
    /// The unit its value is in, empty for none.
    pub unit: &'static str,
    /// The section of the text form it is written under.
    pub section: &'static str,
    /// What it changes.
    pub what: &'static str,
}

const fn num(
    knob: Knob,
    name: &'static str,
    default: f32,
    (min, max): (f32, f32),
    unit: &'static str,
    section: &'static str,
    what: &'static str,
) -> KnobSpec {
    KnobSpec {
        knob,
        name,
        default,
        min,
        max,
        switch: false,
        unit,
        section,
        what,
    }
}

const fn switch(
    knob: Knob,
    name: &'static str,
    on: bool,
    section: &'static str,
    what: &'static str,
) -> KnobSpec {
    KnobSpec {
        knob,
        name,
        default: if on { 1.0 } else { 0.0 },
        min: 0.0,
        max: 1.0,
        switch: true,
        unit: "",
        section,
        what,
    }
}

/// How many knobs there are.
pub const KNOB_COUNT: usize = 102;

/// Every knob, in [`Knob`]'s order.
pub const KNOBS: [KnobSpec; KNOB_COUNT] = [
    switch(Knob::Ground, "ground", true, "ground", "the drawn ground at all: off draws retail's landscape as recorded"),
    num(Knob::Detail, "detail", 1.0, (0.25, 4.0), "", "ground", "how densely the drawn ground is meshed: twice as many triangles a side at 2"),
    num(Knob::Round, "round", 1.0, (0.0, 2.0), "", "walkable", "the share of the smooth surface through retail's vertices kept on walkable ground: 0 is retail's flat facets"),
    num(Knob::WalkCap, "walk_cap", 1.0, (0.0, 4.0), "m", "walkable", "the most walkable ground is drawn off retail's surface, reached softly"),
    num(Knob::WalkLowpass, "walk_lowpass", 2.0, (0.0, 8.0), "m", "walkable", "the width of drawn ground a body's height is averaged over, its stance and a little more: 0 stands it on the ground under its middle"),
    num(Knob::Micro, "micro", 0.05, (0.0, 0.3), "m", "walkable", "the height of the fine relief bodies ignore"),
    num(Knob::MicroScale, "micro_scale", 0.7, (0.1, 4.0), "m", "walkable", "the size of the fine relief's bumps"),
    num(Knob::PinFade, "pin_fade", 8.0, (0.0, 24.0), "m", "pins", "how far the offset takes to rise from zero at a building, a static object, a road or water"),
    num(Knob::PinBuildingMargin, "pin_building_margin", 1.0, (0.0, 6.0), "m", "pins", "the ground kept at retail's height round a building's footprint"),
    num(Knob::PinStaticMargin, "pin_static_margin", 0.5, (0.0, 6.0), "m", "pins", "the ground kept at retail's height round a static object's footprint"),
    switch(Knob::PinRoads, "pin_roads", true, "pins", "roads kept at retail's height"),
    switch(Knob::PinWater, "pin_water", true, "pins", "water and the shore kept at retail's height"),
    num(Knob::PinWaterLevel, "pin_water_level", 0.25, (0.0, 1.0), "", "pins", "how much water a point holds before it is kept at retail's height: lower pins more of the shore"),
    switch(Knob::Cliffs, "cliffs", true, "cliffs", "rock bands on unwalkable ground"),
    num(Knob::CliffCap, "cliff_cap", 4.0, (0.0, 16.0), "m", "cliffs", "the most a rock band leaves retail's surface"),
    num(Knob::CliffCapTall, "cliff_cap_tall", 8.0, (0.0, 24.0), "m", "cliffs", "the same on faces taller than cliff_tall_relief"),
    num(Knob::CliffTallRelief, "cliff_tall_relief", 80.0, (0.0, 600.0), "m", "cliffs", "how tall a face is before it takes cliff_cap_tall"),
    num(Knob::CliffEdge, "cliff_edge", 1.5, (0.0, 8.0), "m", "cliffs", "the unwalkable ground kept at retail's surface next to walkable ground, where bodies stand at the brink"),
    num(Knob::CliffGrowth, "cliff_growth", 0.5, (0.05, 4.0), "m/m", "cliffs", "how fast the bands' room grows past that edge, meters a meter"),
    num(Knob::CliffBand, "cliff_band", 2.0, (0.5, 6.0), "x cap", "cliffs", "a band's height, as a multiple of its cap"),
    num(Knob::CliffRiser, "cliff_riser", 0.33, (0.05, 0.95), "", "cliffs", "the share of a band that is its riser: lower is sheerer"),
    num(Knob::CliffNoise, "cliff_noise", 0.35, (0.0, 1.0), "", "cliffs", "how much the bands' edges wander, as a share of a band"),
    num(Knob::CliffNoiseScale, "cliff_noise_scale", 6.0, (1.0, 48.0), "m", "cliffs", "how far apart the bands' edges wander"),
    switch(Knob::BodyShift, "body_shift", true, "bodies", "bodies, items, scenery and the camera drawn on the drawn ground; off leaves them at retail's heights"),
    num(Knob::ShiftFade, "shift_fade", 3.0, (0.0, 20.0), "m", "bodies", "the height over retail's ground a thing's shift starts to fade at"),
    num(Knob::ShiftGone, "shift_gone", 6.0, (0.0, 40.0), "m", "bodies", "the height the shift is gone at"),
    num(Knob::CameraClearance, "camera_clearance", 0.3, (0.0, 2.0), "m", "bodies", "the least room the camera keeps over the drawn ground"),
    switch(Knob::Level, "level", true, "bodies", "the fine relief and the terraces leveled under each body"),
    num(Knob::LevelRadius, "level_radius", 0.9, (0.0, 4.0), "m", "bodies", "the radius of the level ground under a body"),
    num(Knob::LevelFade, "level_fade", 0.6, (0.0, 4.0), "m", "bodies", "the width it fades back over"),
    switch(Knob::Terraces, "terraces", false, "terraces", "small terraces on steep walkable ground"),
    num(Knob::TerraceBand, "terrace_band", 0.9, (0.2, 4.0), "m", "terraces", "a terrace's height"),
    num(Knob::TerraceCap, "terrace_cap", 0.45, (0.0, 2.0), "m", "terraces", "the most a terrace leaves retail's surface"),
    num(Knob::TerraceFrom, "terrace_from", 30.0, (0.0, 50.0), "deg", "terraces", "the slope terraces start at, fading in over 5 degrees"),
    num(Knob::NearBlocks, "near_blocks", 3.0, (1.0, 6.0), "blocks", "far", "how far round the viewer's block the near offset is worked out"),
    num(Knob::MorphStart, "morph_start", 300.0, (0.0, 3000.0), "m", "far", "where the near ground starts to give way to the far"),
    num(Knob::MorphEnd, "morph_end", 500.0, (0.0, 3000.0), "m", "far", "where it has given way"),
    switch(Knob::Far, "far", true, "far", "ground drawn past retail's landscape, to the horizon"),
    num(Knob::FarRound, "far_round", 1.0, (0.0, 2.0), "", "far", "the share of the smooth surface kept far off, uncapped"),
    num(Knob::FarDetail, "far_detail", 1.5, (0.0, 12.0), "m", "far", "the height of the eroded relief far off"),
    num(Knob::FarDetailScale, "far_detail_scale", 96.0, (8.0, 1000.0), "m", "far", "the size of that relief"),
    num(Knob::Horizon, "horizon", 30000.0, (3000.0, 60000.0), "m", "far", "how far the ground is drawn"),
    num(Knob::Haze, "haze", 1.0, (0.0, 4.0), "", "far", "how thick the air over the far ground is: 1 takes the haze's full depth near the horizon, 2 twice it"),
    switch(Knob::FarTrees, "far_trees", true, "far", "the art pack's trees drawn far off, past the blocks round the viewer, as their far pictures, where retail's scenery places them"),
    num(Knob::TreeReach, "tree_reach", 6000.0, (0.0, 8000.0), "m", "far", "how far from the eye the far trees are drawn"),
    num(Knob::TreeThinFrom, "tree_thin_from", 3000.0, (0.0, 8000.0), "m", "far", "how far every far tree is drawn: past it they thin out, fewer and fewer to the reach"),
    num(Knob::TreeWiden, "tree_widen", 0.5, (0.0, 1.0), "", "far", "how much of a thinned forest's coverage the trees left standing make up by standing wider: 1 all of it, up to half as wide again"),
    num(Knob::TreeSoften, "tree_soften", 1.0, (0.0, 3.0), "", "far", "how much softer the far trees are drawn than their size on screen calls for, in halvings of their pictures' detail: up, still more under a still camera and blurrier; 0 sharpest, their finest leaves stirring"),
    num(Knob::ForestCover, "forest_cover", 1.0, (0.0, 2.0), "", "far", "how strongly the forest cover tints the far ground where the far trees thin out and past them: 0 none"),
    num(Knob::ForestCanopy, "forest_canopy", 0.5, (0.0, 1.0), "", "far", "how high the forest cover raises the far ground at the skyline, a share of its trees' height"),
    switch(Knob::Slope, "slope", true, "slope", "scree and rock laid on the ground by its drawn slope"),
    num(Knob::ScreeFrom, "scree_from", 28.0, (0.0, 80.0), "deg", "slope", "the slope scree is laid from"),
    num(Knob::ScreeBlend, "scree_blend", 6.0, (0.5, 30.0), "deg", "slope", "the slopes scree blends in over"),
    num(Knob::RockFrom, "rock_from", 36.0, (0.0, 80.0), "deg", "slope", "the slope rock is laid from"),
    num(Knob::RockBlend, "rock_blend", 6.0, (0.5, 30.0), "deg", "slope", "the slopes rock blends in over"),
    num(Knob::SlopeStrength, "slope_strength", 1.0, (0.0, 1.0), "", "slope", "how much of the scree and rock is laid over the ground's own material"),
    num(Knob::SlopeSmooth, "slope_smooth", 1.0, (0.0, 1.0), "", "slope", "how much the slope that lays scree and rock is the smooth surface's: 1 lays them in smooth sweeps, 0 by the drawn ground's own facets and bands"),
    num(Knob::SlopeOnSnow, "slope_on_snow", 0.25, (0.0, 1.0), "", "slope", "how much scree and rock show through snow and ice on steep ground: 0 keeps the snow whole"),
    num(Knob::ScreeGround, "scree_ground", 14.0, (0.0, 31.0), "type", "slope", "the terrain type whose material scree is drawn with: 14 mossy rock, 8 patchy dirt, 12 rocky sand"),
    num(Knob::RockGround, "rock_ground", 0.0, (0.0, 31.0), "type", "slope", "the terrain type whose material rock is drawn with: 0 barren rock"),
    switch(Knob::Water, "water", true, "water", "the water drawn as water, with ripples, reflections and a bed seen through it: off lays retail's water picture as the ground"),
    num(Knob::WaterColor, "water_color", 1.0, (0.0, 2.0), "", "water", "the water's own color, as a share of retail's water picture under the same light"),
    num(Knob::WaterClarity, "water_clarity", 1.6, (0.1, 20.0), "m", "water", "how far the eye sees into the water: the bed fades into the water's own color over about this much of it"),
    num(Knob::WaterDepth, "water_depth", 2.5, (0.0, 10.0), "m", "water", "the deepest the bed under the water lies below its surface"),
    num(Knob::WaterShelf, "water_shelf", 30.0, (1.0, 200.0), "m", "water", "how far from the shore the bed reaches that depth"),
    num(Knob::RippleScale, "ripple_scale", 1.0, (0.2, 5.0), "", "water", "the ripples' size: 2 twice as large"),
    num(Knob::RippleSpeed, "ripple_speed", 1.0, (0.0, 4.0), "", "water", "how fast the ripples move: 0 holds them still"),
    num(Knob::RippleStrength, "ripple_strength", 1.0, (0.0, 3.0), "", "water", "how much the ripples bend the surface and break up its reflections"),
    num(Knob::RippleCalm, "ripple_calm", 120.0, (10.0, 2000.0), "m", "water", "how far from the eye the ripples have calmed, so far water lies smooth and mirrors the sky"),
    num(Knob::WaterRings, "water_rings", 1.0, (0.0, 3.0), "", "water", "how strongly the water rings round a body standing in it: 0 none"),
    num(Knob::WaterReflection, "water_reflection", 1.0, (0.0, 1.5), "", "water", "how strongly the water reflects the sky and the land"),
    num(Knob::ShoreSoft, "shore_soft", 1.5, (0.0, 8.0), "m", "water", "how far from its edge the water takes to come in: larger is a softer shore"),
    num(Knob::ShoreWet, "shore_wet", 0.35, (0.0, 1.0), "", "water", "how much darker the wet ground along the water is: where retail lays water above the water's edge, and a strip of any ground beside it"),
    num(Knob::WaterBed, "water_bed", 11.0, (0.0, 31.0), "type", "water", "the terrain type whose material the bed is drawn with away from the shore (the shallows keep the bank's own ground): 11 gray sand, 5 mud, 8 patchy dirt, 12 rocky sand"),
    num(Knob::BankGreen, "bank_green", 0.7, (0.0, 1.0), "", "water", "how much grass grows in over a muddy bank, in patches, more the farther from the water: 0 leaves retail's ring of mud"),
    num(Knob::BankGrass, "bank_grass", 9.0, (0.0, 31.0), "type", "water", "the terrain type grown in over a muddy bank where no other ground lies beside it: 9 patchy grassland, 1 grassland, 3 lush grass"),
    num(Knob::View, "view", 0.0, (0.0, 4.0), "", "views", "a diagnostic view: 1 the offset (red raised, blue lowered, 1 m full), 2 the pins (magenta pinned), 3 retail's slope (pale under 20 degrees, olive, yellow, orange to 48.4, magenta unwalkable), 4 walkable and unwalkable ground"),
    num(Knob::PackExposure, "pack_exposure", 1.0, (0.0, 1.0), "", "light", "how much an art pack's models' lit surfaces and its materials count, against the world's own surfaces, when the outdoor exposure is held to the ordinary picture"),
    num(Knob::LeafExposure, "leaf_exposure", 0.1, (0.0, 1.0), "", "light", "how much an art pack's leaves count there: their crowns' own shading darkens them where the old light knows nothing of it, and counted wholly a dense crown would raise the whole frame"),
    num(Knob::GroundExposure, "ground_exposure", 1.0, (0.0, 1.0), "", "light", "how much an art pack's ground counts there"),
    num(Knob::PackGlow, "pack_glow", 1.0, (0.0, 1.0), "", "light", "how much of the light the retail object's own surfaces give off (its luminous leaves, a glowing stone) an art pack's model keeps, whatever the time of day: 0 lights it by the sun and the sky alone"),
    num(Knob::LeafInnerSky, "leaf_inner_sky", 0.15, (0.0, 1.0), "", "light", "the share of the open sky a leaf deep inside an art pack's crown still sees between the leaves round it"),
    num(Knob::LeafScatter, "leaf_scatter", 0.4, (0.0, 1.0), "", "light", "the share of the sun a leaf on the outside of an art pack's crown keeps in shade"),
    num(Knob::LeafScatterDeep, "leaf_scatter_deep", 0.5, (0.0, 1.0), "", "light", "how much of that a leaf deep inside its crown takes"),
    switch(Knob::Sky, "sky", true, "sky", "the Remastered sky: retail's clouds, sun, moons and stars drawn again at modern quality over the physical sky; off draws retail's own"),
    switch(Knob::Clouds, "clouds", true, "sky", "the Remastered clouds, laid where retail's cloud picture lays them and drifting with it; off draws retail's cloud deck"),
    num(Knob::CloudSteps, "cloud_steps", 12.0, (1.0, 64.0), "", "sky", "how many steps the clouds are drawn with through their depth: more is fuller and softer, and costs more; 1 is a flat layer"),
    num(Knob::CloudResolution, "cloud_resolution", 0.5, (0.25, 1.0), "", "sky", "the share of the frame's pixels each way the clouds are worked out at before they are enlarged to it"),
    num(Knob::CloudCover, "cloud_cover", 1.0, (0.0, 2.0), "", "sky", "how much of the sky the clouds cover, against retail's own cloud picture: 0 none, 2 twice the cover"),
    num(Knob::CloudHeight, "cloud_height", 1500.0, (200.0, 6000.0), "m", "sky", "how high over the eye the clouds' base lies"),
    num(Knob::CloudDepth, "cloud_depth", 900.0, (50.0, 4000.0), "m", "sky", "how thick the clouds are, base to top"),
    num(Knob::CloudScale, "cloud_scale", 1.0, (0.25, 4.0), "", "sky", "how large retail's cloud picture is laid over the sky: 1 as large overhead as retail draws it, 2 twice as large"),
    num(Knob::CloudDetail, "cloud_detail", 1.0, (0.0, 2.0), "", "sky", "how much fine detail breaks up the clouds' edges and faces: 0 retail's soft shapes alone"),
    num(Knob::CloudSpeed, "cloud_speed", 1.0, (0.0, 4.0), "", "sky", "how fast the clouds drift, a share of retail's drift: 0 holds them still"),
    num(Knob::CloudLight, "cloud_light", 1.0, (0.0, 3.0), "", "sky", "how brightly the sun lights the clouds' faces and their rims toward it"),
    num(Knob::CloudShade, "cloud_shade", 1.0, (0.0, 2.0), "", "sky", "how much darker the clouds' bases are than their tops: 0 lit evenly"),
    num(Knob::CloudShadows, "cloud_shadows", 0.5, (0.0, 1.0), "", "sky", "how much the clouds shade the land under them: 0 none, 1 their full shadow"),
    num(Knob::SunSize, "sun_size", 1.0, (0.2, 6.0), "deg", "sky", "the sun's disk across"),
    num(Knob::SunGlow, "sun_glow", 1.0, (0.0, 3.0), "", "sky", "how strong the glow about the sun is"),
    num(Knob::MoonSize, "moon_size", 1.0, (0.25, 3.0), "", "sky", "the moons' size, a share of retail's"),
    num(Knob::Stars, "stars", 1.0, (0.0, 4.0), "", "sky", "how bright the stars are: 0 none"),
    num(Knob::StarTwinkle, "star_twinkle", 1.0, (0.0, 2.0), "", "sky", "how much the stars twinkle as the clock runs: 0 holds them steady"),
];

impl Knob {
    /// The knob's spec.
    #[must_use]
    pub fn spec(self) -> &'static KnobSpec {
        &KNOBS[self as usize]
    }

    /// The knob named `name` in the text form.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        KNOBS.iter().find(|k| k.name == name).map(|k| k.knob)
    }
}

/// A value of every knob.
#[derive(Debug, Clone, PartialEq)]
pub struct Knobs {
    values: [f32; KNOB_COUNT],
}

impl Default for Knobs {
    fn default() -> Self {
        Self {
            values: KNOBS.map(|k| k.default),
        }
    }
}

impl Knobs {
    /// The value of `knob`.
    #[must_use]
    pub fn get(&self, knob: Knob) -> f32 {
        self.values[knob as usize]
    }

    /// Whether the switch `knob` is on (any value from a half up).
    #[must_use]
    pub fn on(&self, knob: Knob) -> bool {
        self.get(knob) >= 0.5
    }

    /// Sets `knob` to `value`, brought into its range; a switch takes 0 or 1. Returns whether
    /// the value had to be brought into the range. A value that is not a number leaves the knob
    /// as it was.
    pub fn set(&mut self, knob: Knob, value: f32) -> bool {
        if value.is_nan() {
            return true;
        }
        let spec = knob.spec();
        let kept = if spec.switch {
            if value >= 0.5 {
                1.0
            } else {
                0.0
            }
        } else {
            value.clamp(spec.min, spec.max)
        };
        self.values[knob as usize] = kept;
        kept.to_bits() != value.to_bits()
    }

    /// The knobs `text` sets, over the defaults, and a note for each line that could not be
    /// taken as it was written.
    #[must_use]
    pub fn parse(text: &str) -> (Self, Vec<String>) {
        let mut knobs = Self::default();
        let notes = knobs.apply(text);
        (knobs, notes)
    }

    /// Sets the knobs `text` names, leaving the rest, and a note for each line that could not be
    /// taken as it was written.
    pub fn apply(&mut self, text: &str) -> Vec<String> {
        let mut notes = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() || line.starts_with('[') {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                notes.push(format!("line {}: not `name = value`: {raw}", n + 1));
                continue;
            };
            let name = name.trim();
            let Some(knob) = Knob::named(name) else {
                notes.push(format!("line {}: no knob is named {name}", n + 1));
                continue;
            };
            let value = value.trim();
            let parsed = match value {
                "true" | "on" => Some(1.0),
                "false" | "off" => Some(0.0),
                v => v.parse::<f32>().ok(),
            };
            let Some(v) = parsed.filter(|v| v.is_finite()) else {
                notes.push(format!("line {}: {name} cannot be {value}", n + 1));
                continue;
            };
            let spec = knob.spec();
            let as_written = if spec.switch {
                v == 0.0 || v == 1.0
            } else {
                (spec.min..=spec.max).contains(&v)
            };
            self.set(knob, v);
            if !as_written {
                notes.push(format!(
                    "line {}: {name} = {value} is outside {} to {}; {} is used",
                    n + 1,
                    spec.min,
                    spec.max,
                    self.get(knob)
                ));
            }
        }
        notes
    }

    /// Every knob in the text form, by section, each with what it changes, its unit, its range
    /// and its default: a file [`Self::parse`] reads back as these knobs.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let mut section = "";
        for spec in &KNOBS {
            if spec.section != section {
                section = spec.section;
                if !out.is_empty() {
                    out.push('\n');
                }
                let _ = writeln!(out, "[{section}]");
            }
            let value = self.get(spec.knob);
            if spec.switch {
                let _ = writeln!(out, "# {} (default {})", spec.what, spec.default >= 0.5);
                let _ = writeln!(out, "{} = {}", spec.name, value >= 0.5);
            } else {
                let unit = if spec.unit.is_empty() {
                    String::new()
                } else {
                    format!(" {}", spec.unit)
                };
                let _ = writeln!(
                    out,
                    "# {}; {} to {}{unit} (default {})",
                    spec.what, spec.min, spec.max, spec.default
                );
                let _ = writeln!(out, "{} = {value}", spec.name);
            }
        }
        out
    }

    /// The knobs whose values differ from `other`'s.
    #[must_use]
    pub fn changed_from(&self, other: &Self) -> Vec<Knob> {
        KNOBS
            .iter()
            .filter(|k| self.get(k.knob).to_bits() != other.get(k.knob).to_bits())
            .map(|k| k.knob)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the drawn ground's tuning, an art-pack presentation's own)
    use super::*;

    #[test]
    fn every_knob_stands_at_its_own_place_in_the_table_with_a_name_of_its_own() {
        for (i, spec) in KNOBS.iter().enumerate() {
            assert_eq!(spec.knob as usize, i, "{}", spec.name);
            assert_eq!(Knob::named(spec.name), Some(spec.knob), "{}", spec.name);
            assert!(
                spec.min <= spec.default && spec.default <= spec.max,
                "{}",
                spec.name
            );
            assert!(!spec.what.is_empty(), "{}", spec.name);
        }
    }

    #[test]
    fn the_knobs_read_back_from_their_text_as_they_were_written() {
        assert_eq!(
            Knobs::parse(&Knobs::default().to_text()),
            (Knobs::default(), Vec::new())
        );
        let mut k = Knobs::default();
        k.set(Knob::WalkCap, 0.3);
        k.set(Knob::Cliffs, 0.0);
        k.set(Knob::Terraces, 1.0);
        k.set(Knob::CliffNoiseScale, 11.25);
        k.set(Knob::Horizon, 22_000.0);
        let (back, notes) = Knobs::parse(&k.to_text());
        assert_eq!(back, k);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(back.changed_from(&Knobs::default()).len(), 5);
    }

    #[test]
    fn a_line_the_knobs_cannot_take_is_noted_and_the_rest_are_taken() {
        let text = "[walkable]\nwalk_cap = 0.5 # half a meter\nno_such_knob = 3\nmicro = lots\ncliff_cap = 99\nterraces = on\n";
        let (k, notes) = Knobs::parse(text);
        assert!((k.get(Knob::WalkCap) - 0.5).abs() < 1e-6);
        assert!(k.on(Knob::Terraces));
        assert!(
            (k.get(Knob::CliffCap) - 16.0).abs() < 1e-6,
            "brought into its range"
        );
        assert!((k.get(Knob::Micro) - Knob::Micro.spec().default).abs() < 1e-6);
        assert_eq!(notes.len(), 3, "{notes:?}");
    }
}
