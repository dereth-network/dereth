//! Immutable art, text metrics and content belonging to one classic interface.
use crate::{
    art::ClassicArt, help::Book, panels::pregame::data::CreationData, renderer::FontMetrics,
};
use std::{rc::Rc, sync::Arc};

#[derive(Clone, Debug)]
pub struct Resources {
    pub art: Option<Arc<ClassicArt>>,
    pub fonts: FontMetrics,
    pub help: Option<Arc<Book>>,
    pub creation: Result<Rc<CreationData>, String>,
}
impl Default for Resources {
    fn default() -> Self {
        Self {
            art: None,
            fonts: FontMetrics::default(),
            help: None,
            creation: Err("World creation tables unavailable".into()),
        }
    }
}
impl Resources {
    pub fn new(
        art: Arc<ClassicArt>,
        creation: Result<Rc<CreationData>, String>,
        help: Option<Arc<Book>>,
    ) -> Self {
        Self {
            art: Some(art),
            creation,
            help,
            fonts: FontMetrics::default(),
        }
    }
}
