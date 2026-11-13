use gpui_kit::*;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

pub struct CoverImages {
    entries: HashMap<Resource, ImageCacheItem>,
    recent: VecDeque<Resource>,
}

impl CoverImages {
    pub fn new(cx: &mut App) -> Entity<Self> {
        let entity = cx.new(|_| Self {
            entries: HashMap::new(),
            recent: VecDeque::new(),
        });
        cx.observe_release(&entity, |cache, cx| {
            for (_, item) in cache.entries.drain() {
                if let Some(Ok(image)) = item.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        entity
    }
}

impl ImageCache for CoverImages {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.recent.retain(|old| old != resource);
        self.recent.push_back(resource.clone());
        if self.recent.len() > 64
            && let Some(old) = self.recent.pop_front()
            && let Some(item) = self.entries.remove(&old)
            && let Some(Ok(image)) = item.get()
        {
            cx.drop_image(image, None);
        }
        self.entries
            .entry(resource.clone())
            .or_insert_with(|| ImageCacheItem::new(resource, cx))
            .use_image(window)
    }
}

pub struct BookImages {
    recent: VecDeque<Arc<Image>>,
}

enum DecodeImage {}

impl Asset for DecodeImage {
    type Source = Arc<Image>;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let renderer = cx.svg_renderer();
        async move { source.to_image_data(renderer).map_err(Into::into) }
    }
}

impl BookImages {
    pub fn new(cx: &mut App) -> Entity<Self> {
        let entity = cx.new(|_| Self {
            recent: VecDeque::new(),
        });
        cx.observe_release(&entity, |cache, cx| cache.clear(cx))
            .detach();
        entity
    }

    fn evict(image: &Arc<Image>, cx: &mut App) {
        if let Some(Ok(decoded)) = cx.fetch_asset::<DecodeImage>(image) {
            cx.drop_image(decoded, None);
        }
        cx.remove_asset::<DecodeImage>(image);
    }

    pub fn clear(&mut self, cx: &mut App) {
        for image in self.recent.drain(..) {
            Self::evict(&image, cx);
        }
    }

    pub fn source(cache: &Entity<Self>, image: Arc<Image>) -> ImageSource {
        let cache = cache.clone();
        ImageSource::Custom(Arc::new(move |window, cx| {
            cache.update(cx, |cache, cx| {
                cache.recent.retain(|old| old.id() != image.id());
                cache.recent.push_back(image.clone());
                if cache.recent.len() > 32
                    && let Some(old) = cache.recent.pop_front()
                {
                    Self::evict(&old, cx);
                }
                window.use_asset::<DecodeImage>(&image, cx)
            })
        }))
    }
}
