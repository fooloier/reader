use super::images::BookImages;
use gpui_kit::{
    component::{
        ActiveTheme,
        text::{TextView, TextViewState, TextViewStyle},
    },
    prelude::*,
    *,
};
use reader_core::{EpubSettings, Location};
use reader_runtime::epub::EpubBook;
use std::{cell::Cell, collections::HashMap, rc::Rc, sync::Arc};

pub struct EpubPane {
    pub document: EpubBook,
    pub chapter: usize,
    pub settings: EpubSettings,
    pub scroll: ScrollHandle,
    texts: Vec<Entity<TextViewState>>,
    images: Arc<HashMap<String, Arc<Image>>>,
    image_cache: Entity<BookImages>,
    pending: Option<usize>,
    restore_target: Rc<Cell<Option<usize>>>,
    restore_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    last_anchor: String,
    navigation: u64,
}

impl EpubPane {
    pub fn new(
        document: EpubBook,
        settings: EpubSettings,
        location: Option<Location>,
        cx: &mut Context<Self>,
    ) -> Self {
        let images = document
            .images
            .iter()
            .map(|(key, bytes)| {
                let format = if bytes.starts_with(b"\x89PNG") {
                    ImageFormat::Png
                } else if bytes.starts_with(b"\xff\xd8") {
                    ImageFormat::Jpeg
                } else if bytes.starts_with(b"GIF") {
                    ImageFormat::Gif
                } else if key.ends_with(".svg") {
                    ImageFormat::Svg
                } else {
                    ImageFormat::Webp
                };
                (
                    key.clone(),
                    Arc::new(Image::from_bytes(format, bytes.clone())),
                )
            })
            .collect();
        let (chapter, block) = match location {
            Some(Location::Epub { href, anchor }) => document
                .destination(&format!(
                    "{href}#{}",
                    reader_runtime::epub::encode_component(&anchor)
                ))
                .unwrap_or((0, 0)),
            _ => (0, 0),
        };
        let mut pane = Self {
            document,
            chapter,
            settings,
            scroll: ScrollHandle::new(),
            texts: vec![],
            images: Arc::new(images),
            image_cache: BookImages::new(cx),
            pending: Some(block),
            restore_target: Rc::new(Cell::new(Some(block))),
            restore_bounds: Rc::new(Cell::new(None)),
            last_anchor: String::new(),
            navigation: 0,
        };
        pane.rebuild(cx);
        pane
    }
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.image_cache.update(cx, |cache, cx| cache.clear(cx));
        self.texts = self.document.chapters[self.chapter]
            .blocks
            .iter()
            .map(|b| cx.new(|cx| TextViewState::markdown(&b.markdown, cx).selectable(true)))
            .collect();
        self.scroll = ScrollHandle::new();
        cx.notify();
    }
    pub fn location(&self) -> Location {
        let chapter = &self.document.chapters[self.chapter];
        let index = self
            .restore_target
            .get()
            .or(self.pending)
            .unwrap_or_else(|| self.scroll.top_item())
            .min(chapter.blocks.len().saturating_sub(1));
        Location::Epub {
            href: chapter.href.clone(),
            anchor: chapter
                .blocks
                .get(index)
                .map(|b| b.anchor.clone())
                .unwrap_or_default(),
        }
    }
    pub fn navigate(&mut self, href: &str, cx: &mut Context<Self>) {
        if let Some((chapter, block)) = self.document.destination(href) {
            self.navigation += 1;
            if self.chapter != chapter {
                self.chapter = chapter;
                self.rebuild(cx);
            }
            self.pending = Some(block);
            self.restore_target.set(Some(block));
            self.restore_bounds.set(None);
            self.changed(cx);
            cx.notify();
        }
    }
    pub fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let next = if forward {
            (self.chapter + 1).min(self.document.chapters.len() - 1)
        } else {
            self.chapter.saturating_sub(1)
        };
        if next == self.chapter {
            return;
        }
        let href = self.document.chapters[next].href.clone();
        self.navigate(&href, cx);
    }
    pub fn configure(&mut self, settings: EpubSettings, cx: &mut Context<Self>) {
        if self.settings == settings {
            return;
        }
        self.navigation += 1;
        self.pending = self.restore_target.get().or(Some(self.scroll.top_item()));
        self.restore_target.set(self.pending);
        self.restore_bounds.set(None);
        self.settings = settings;
        for text in &self.texts {
            text.update(cx, |v, cx| v.invalidate_inline_layout(cx));
        }
        cx.notify();
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        let location = self.location();
        let key = format!("{location:?}");
        if self.last_anchor != key {
            self.last_anchor = key;
            cx.emit(location);
        }
    }
}
impl EventEmitter<Location> for EpubPane {}

impl Render for EpubPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(block) = self.pending.take() {
            let entity = cx.weak_entity();
            let navigation = self.navigation;
            window.on_next_frame(move |_, cx| {
                let _ = entity.update(cx, |this, cx| {
                    if this.navigation != navigation || this.restore_target.get() != Some(block) {
                        return;
                    }
                    this.scroll.scroll_to_top_of_item(block);
                    this.changed(cx);
                    cx.notify();
                });
            });
        }
        let owner = cx.weak_entity();
        let children = self
            .texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let images = self.images.clone();
                let cache = self.image_cache.clone();
                let owner = owner.clone();
                crate::controls::observed(
                    div()
                        .id(SharedString::from(format!("epub-block-{i}")))
                        .w_full()
                        .max_w(px(self.settings.width))
                        .flex_shrink_0()
                        .px_6()
                        .pb(rems(self.settings.paragraph_gap))
                        .child(
                            TextView::new(text)
                                .w_full()
                                .font_family(self.settings.font.clone())
                                .text_size(px(self.settings.size))
                                .line_height(px(self.settings.size * self.settings.line_height))
                                .style(
                                    TextViewStyle::default()
                                        .paragraph_gap(rems(self.settings.paragraph_gap)),
                                )
                                .image_source(move |uri| {
                                    images
                                        .get(uri.as_ref())
                                        .cloned()
                                        .map(|image| BookImages::source(&cache, image))
                                        .unwrap_or_else(|| {
                                            ImageSource::from(Arc::new(Image::from_bytes(
                                                ImageFormat::Png,
                                                vec![],
                                            )))
                                        })
                                })
                                .on_link_click(move |href, _, _, cx| {
                                    let _ = owner.update(cx, |this, cx| this.navigate(href, cx));
                                }),
                        ),
                )
            })
            .collect::<Vec<_>>();
        let scroll = self.scroll.clone();
        let target = self.restore_target.clone();
        let previous = self.restore_bounds.clone();
        let observer = canvas(
            move |_, window, _| {
                if let Some(block) = target.get()
                    && let Some(bounds) = scroll.bounds_for_item(block)
                    && previous.get() != Some(bounds)
                {
                    previous.set(Some(bounds));
                    let scroll = scroll.clone();
                    window.on_next_frame(move |window, _| {
                        if target.get() == Some(block) {
                            scroll.scroll_to_top_of_item(block);
                            window.refresh();
                        }
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let content = crate::controls::observed(
            div()
                .id("epub-scroll")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .flex()
                .flex_col()
                .items_center()
                .py_6()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .font_family(self.settings.font.clone())
                .text_size(px(self.settings.size))
                .line_height(relative(self.settings.line_height))
                .on_scroll(cx.listener(|this, _, window, cx| {
                    this.restore_target.set(None);
                    let entity = cx.weak_entity();
                    window.on_next_frame(move |_, cx| {
                        let _ = entity.update(cx, |this, cx| this.changed(cx));
                    });
                }))
                .children(children)
                .when(self.texts.is_empty(), |view| {
                    view.child("本章没有正文，可前往下一章")
                }),
        );
        div().relative().size_full().child(content).child(observer)
    }
}
