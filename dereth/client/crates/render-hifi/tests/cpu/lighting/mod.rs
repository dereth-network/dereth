//! The light per pixel: which point lights the relight takes, the rooms' own light, the
//! exposure, how bright the outdoors is drawn, how the day's weather shares the light between
//! the sun and the sky, what an art pack's leaves carry to the light, and how an art pack's
//! ground takes the old light.

mod daylight;
mod exposure;
mod foliage;
mod ground;
mod outdoor;
mod point_lights;
mod rooms;
mod traced;
