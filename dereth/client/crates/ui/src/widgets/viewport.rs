use super::*;

#[derive(Debug, Default)]
pub struct Viewport {
    /// `CreatureMode`, opaque here.
    pub creature_mode: u32,
}

pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
    Box::new(Viewport::default())
}

impl Element for Viewport {}
