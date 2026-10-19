use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use gpui_kit::*;

pub const DEFAULT_CAPACITY: usize = 100;

pub struct LruImageCache {
    capacity: usize,
    order: VecDeque<u64>,
    items: HashMap<u64, ImageCacheItem>,
}

impl LruImageCache {
    pub fn new(capacity: usize, cx: &mut App) -> Entity<Self> {
        let entity = cx.new(|_cx| Self {
            capacity: capacity.max(1),
            order: VecDeque::new(),
            items: HashMap::new(),
        });

        // Releasing the cache (grid unmounted) must free every texture it holds.
        cx.observe_release(&entity, |cache, cx| {
            for (_, item) in std::mem::take(&mut cache.items) {
                if let Some(Ok(image)) = item.get() {
                    cx.drop_image(image, None);
                }
            }
            cache.order.clear();
        })
        .detach();

        entity
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
    }

    fn touch(&mut self, key: u64) {
        if let Some(pos) = self.order.iter().position(|k| *k == key) {
            let key = self
                .order
                .remove(pos)
                .expect("position came from this deque");
            self.order.push_back(key);
        }
    }

    /// Drop the oldest textures until the cache fits inside its capacity.
    fn evict(&mut self, window: &mut Window, cx: &mut App) {
        while self.items.len() > self.capacity {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            let Some(item) = self.items.remove(&oldest) else {
                continue;
            };
            if let Some(Ok(image)) = item.get() {
                cx.drop_image(image, Some(window));
            }
        }
    }
}

impl ImageCache for LruImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = hash(resource);

        if self.items.contains_key(&key) {
            self.touch(key);
            return self.items.get(&key)?.use_image(window);
        }

        self.items.insert(key, ImageCacheItem::new(resource, cx));
        self.order.push_back(key);
        self.evict(window, cx);

        self.items.get(&key)?.use_image(window)
    }
}

pub fn lru_image_cache(id: impl Into<ElementId>, capacity: usize) -> ImageCacheElement {
    image_cache(LruImageCacheProvider {
        id: id.into(),
        capacity: capacity.max(DEFAULT_CAPACITY),
    })
}

struct LruImageCacheProvider {
    id: ElementId,
    capacity: usize,
}

impl ImageCacheProvider for LruImageCacheProvider {
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> AnyImageCache {
        let capacity = self.capacity;
        window
            .with_global_id(self.id.clone(), |global_id, window| {
                window.with_element_state::<Entity<LruImageCache>, _>(global_id, |cache, _| {
                    let cache = match cache {
                        Some(cache) => {
                            cache.update(cx, |c, _| {
                                if c.capacity != capacity {
                                    c.set_capacity(capacity);
                                }
                            });
                            cache
                        }
                        None => LruImageCache::new(capacity, cx),
                    };
                    (cache.clone(), cache)
                })
            })
            .into()
    }
}
