pub mod collection;
pub mod tag;
pub mod view;

pub use collection::*;
pub use tag::*;
pub use view::*;

pub struct FileStore;

impl FileStore {
    pub fn new() -> Self {
        Self
    }
}
