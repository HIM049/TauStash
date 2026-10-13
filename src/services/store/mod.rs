pub mod collection;
pub mod tag;
pub mod view;

pub use collection::*;
pub use tag::*;
pub use view::*;

use gpui_kit::*;

#[derive(Clone)]
pub struct AppStores {
    pub views: Entity<ViewStore>,
    pub collections: Entity<CollectionStore>,
    pub tags: Entity<TagStore>,
}

impl AppStores {
    pub fn new(cx: &mut App) -> Self {
        Self {
            views: cx.new(ViewStore::new),
            collections: cx.new(CollectionStore::new),
            tags: cx.new(TagStore::new),
        }
    }
}
