use super::{EpubPane, ReaderState};
use gpui_kit::{
    base::TextSelection,
    component::{
        ActiveTheme, IconName, Sizable,
        button::{Button, ButtonVariant, ButtonVariants},
        popover::Popover,
        spinner::Spinner,
    },
    prelude::*,
    *,
};
use gpui_pdf::PdfView;
use reader_core::BookId;
use reader_runtime::translation::TranslationService;
use std::{ops::Range, sync::Arc};

actions!(reader_translation, [TranslateSelection]);

pub(crate) struct Translations(pub TranslationService);
impl Global for Translations {}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SelectionRange {
    Pdf {
        page: usize,
        runs: Range<usize>,
    },
    Epub {
        chapter: usize,
        block: usize,
        source: Range<usize>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectionSnapshot {
    pub document: BookId,
    pub revision: u64,
    pub text: String,
    pub ranges: Vec<SelectionRange>,
    pub glyph: Bounds<Pixels>,
}
impl SelectionSnapshot {
    fn same_selection(&self, other: &Self) -> bool {
        self.document == other.document && self.text == other.text && self.ranges == other.ranges
    }
}
#[derive(Clone)]
enum Source {
    Pdf(Entity<PdfView>),
    Epub(Entity<EpubPane>),
}
impl Source {
    fn snapshot(&self, document: BookId, cx: &App) -> Option<SelectionSnapshot> {
        match self {
            Self::Pdf(pdf) => {
                let (page, runs, text, glyph) = pdf.read(cx).selection_snapshot()?;
                Some(SelectionSnapshot {
                    document,
                    revision: 0,
                    text,
                    ranges: vec![SelectionRange::Pdf { page, runs }],
                    glyph,
                })
            }
            Self::Epub(pane) => pane.read(cx).selection_snapshot(document, cx),
        }
    }
}
#[derive(Clone)]
pub(crate) enum TranslationState {
    Idle,
    Armed,
    Loading,
    Ready(Arc<str>),
    Failed(String),
}
impl TranslationState {
    fn is_open(&self) -> bool {
        matches!(self, Self::Loading | Self::Ready(_) | Self::Failed(_))
    }
}
pub(crate) struct TranslationOverlay {
    document: BookId,
    source: Option<Source>,
    pub snapshot: Option<SelectionSnapshot>,
    pub state: TranslationState,
    generation: u64,
    revision: u64,
    active: bool,
    viewport: Bounds<Pixels>,
    service: TranslationService,
    subscriptions: Vec<Subscription>,
}
impl TranslationOverlay {
    pub fn new(document: BookId, service: TranslationService) -> Self {
        Self {
            document,
            source: None,
            snapshot: None,
            state: TranslationState::Idle,
            generation: 0,
            revision: 0,
            active: false,
            viewport: Bounds::default(),
            service,
            subscriptions: vec![],
        }
    }
    pub fn attach(&mut self, state: &ReaderState, cx: &mut Context<Self>) {
        self.subscriptions.clear();
        self.source = match state {
            ReaderState::Pdf(pdf) => {
                self.subscriptions
                    .push(cx.observe(pdf, |_, _, cx| cx.notify()));
                Some(Source::Pdf(pdf.clone()))
            }
            ReaderState::Epub(pane) => {
                self.subscriptions
                    .push(cx.observe(pane, |_, _, cx| cx.notify()));
                Some(Source::Epub(pane.clone()))
            }
            _ => None,
        };
        cx.notify();
    }
    pub fn invalidate(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.revision += 1;
        self.snapshot = None;
        self.state = TranslationState::Idle;
        cx.notify();
    }
    pub fn deactivate(&mut self, cx: &mut Context<Self>) {
        self.active = false;
        self.sync(None, self.viewport, cx);
    }
    pub fn activate(&mut self, cx: &mut Context<Self>) {
        if !self.active {
            self.active = true;
            cx.notify();
        }
    }
    #[cfg(all(test, feature = "test-support"))]
    pub fn diagnostics(&self, cx: &App) -> String {
        format!(
            "active={} viewport={:?} source={:?} snapshot={:?}",
            self.active,
            self.viewport,
            self.source
                .as_ref()
                .and_then(|s| s.snapshot(self.document, cx)),
            self.snapshot
        )
    }
    fn sync(
        &mut self,
        snapshot: Option<SelectionSnapshot>,
        viewport: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let mut snapshot = snapshot.filter(|s| self.active && viewport.contains(&s.glyph.center()));
        let same = match (&self.snapshot, &snapshot) {
            (Some(a), Some(b)) => a.same_selection(b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.generation += 1;
            self.revision += 1;
            self.state = if snapshot.is_some() {
                TranslationState::Armed
            } else {
                TranslationState::Idle
            };
        }
        if let Some(snapshot) = &mut snapshot {
            snapshot.revision = self.revision;
        }
        if self.snapshot != snapshot || self.viewport != viewport {
            self.snapshot = snapshot;
            self.viewport = viewport;
            cx.notify();
        }
    }
    pub fn begin(&mut self, cx: &mut Context<Self>) {
        if !self.active || matches!(self.state, TranslationState::Loading) {
            return;
        }
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        let revision = self.revision;
        let reply = self.service.translate(&snapshot.text);
        self.state = TranslationState::Loading;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = reply
                .await
                .unwrap_or_else(|_| Err("翻译服务已停止，请重试。".into()));
            let _ = this.update(cx, |this, cx| {
                if this.active
                    && this.generation == generation
                    && this.revision == revision
                    && matches!(this.state, TranslationState::Loading)
                    && this
                        .source
                        .as_ref()
                        .and_then(|source| source.snapshot(this.document, cx))
                        .zip(this.snapshot.as_ref())
                        .is_some_and(|(current, expected)| current.same_selection(expected))
                {
                    this.state = match result {
                        Ok(text) => TranslationState::Ready(text),
                        Err(error) => TranslationState::Failed(error),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.state = if self.snapshot.is_some() {
            TranslationState::Armed
        } else {
            TranslationState::Idle
        };
        cx.notify();
    }
}

fn preserve_selection(id: &'static str, element: impl IntoElement + 'static) -> impl IntoElement {
    div().relative().child(element).child(
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, cx| {
                TextSelection::preserve_on_mouse_down(id, hitbox, window, cx)
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full(),
    )
}

impl Render for TranslationOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let source = self.source.clone();
        let document = self.document;
        let owner = cx.weak_entity();
        let sample = canvas(
            |bounds, _, _| bounds,
            move |_, viewport, _, cx| {
                let snapshot = source
                    .as_ref()
                    .and_then(|source| source.snapshot(document, cx));
                let owner = owner.clone();
                cx.defer(move |cx| {
                    let _ = owner.update(cx, |this, cx| this.sync(snapshot, viewport, cx));
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let mut root = div().absolute().top_0().left_0().size_full().child(sample);
        if !self.active {
            return root;
        }
        let Some(snapshot) = &self.snapshot else {
            return root;
        };
        let size = window.rem_size() * 1.5;
        let gap = window.rem_size() * 0.25;
        let glyph = snapshot.glyph;
        let left = (glyph.right() - size)
            .max(self.viewport.left() + gap)
            .min((self.viewport.right() - size - gap).max(self.viewport.left()));
        let top = if glyph.top() - size - gap >= self.viewport.top() {
            glyph.top() - size - gap
        } else {
            glyph.bottom() + gap
        };
        let top = top.min((self.viewport.bottom() - size - gap).max(self.viewport.top()));
        let state = self.state.clone();
        let content_owner = cx.entity();
        let popover = Popover::new("selection-translation")
            .open(self.state.is_open())
            .trigger(
                crate::controls::icon(
                    "translate-selection",
                    IconName::Globe,
                    if cfg!(target_os = "macos") {
                        "翻译为简体中文（Cmd+Shift+T）"
                    } else {
                        "翻译为简体中文（Ctrl+Shift+T）"
                    },
                )
                .with_variant(ButtonVariant::Default),
            )
            .on_open_change(cx.listener(|this, open, _, cx| {
                if *open {
                    this.begin(cx);
                } else {
                    this.dismiss(cx);
                }
            }))
            .content(move |_, window, cx| {
                let owner = content_owner.clone();
                let mut content = div()
                    .id("translation-panel")
                    .w_80()
                    .max_w(window.viewport_size().width - window.rem_size() * 3.)
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("简体中文"),
                    );
                content = match &state {
                    TranslationState::Loading => content.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Spinner::new())
                            .child("正在翻译…"),
                    ),
                    TranslationState::Ready(text) => {
                        let copy = text.clone();
                        content
                            .child(crate::controls::observed(
                                div()
                                    .id("translation-result")
                                    .max_h(
                                        (window.viewport_size().height - window.rem_size() * 8.)
                                            .max(window.rem_size() * 4.)
                                            .min(window.rem_size() * 24.),
                                    )
                                    .overflow_y_scroll()
                                    .child(text.to_string()),
                            ))
                            .child(
                                Button::new("copy-translation")
                                    .label("复制译文")
                                    .small()
                                    .ghost()
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy.to_string(),
                                        ))
                                    }),
                            )
                    }
                    TranslationState::Failed(error) => {
                        content.child(div().text_sm().child(error.clone())).child(
                            Button::new("retry-translation")
                                .label("重试")
                                .small()
                                .on_click(move |_, _, cx| {
                                    owner.update(cx, |this, cx| this.begin(cx))
                                }),
                        )
                    }
                    _ => content,
                };
                preserve_selection(
                    "translation-popup-preserve",
                    crate::controls::observed(content),
                )
            });
        root = root.child(
            div()
                .absolute()
                .left(left - self.viewport.left())
                .top(top - self.viewport.top())
                .child(preserve_selection("translation-trigger-preserve", popover)),
        );
        root
    }
}
