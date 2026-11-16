use super::{EpubPane, ReaderState};
use gpui_kit::{
    component::{
        ActiveTheme, IconName, Sizable,
        button::{Button, ButtonVariant, ButtonVariants},
        popover::Popover,
        spinner::Spinner,
    },
    prelude::*,
    *,
};
use gpui_pdf::{PdfEvent, PdfView};
use reader_runtime::translation::TranslationService;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

actions!(reader_translation, [TranslateSelection]);

pub(crate) struct Translations(pub TranslationService);
impl Global for Translations {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SelectionAnchor {
    Pointer(Point<Pixels>),
    Viewport,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectionSnapshot {
    pub text: String,
    pub anchor: SelectionAnchor,
    pub generation: u64,
}
enum Capture {
    Idle,
    Pressed,
    AwaitingPdfText(SelectionAnchor),
    Offered(SelectionSnapshot),
}
#[derive(Clone)]
enum Source {
    Pdf(Entity<PdfView>),
    Epub(Entity<EpubPane>),
}
impl Source {
    fn selected_text(&self, cx: &App) -> Option<String> {
        match self {
            Self::Pdf(pdf) => pdf.read(cx).selection_snapshot().map(|(_, text)| text),
            Self::Epub(pane) => pane.read(cx).selected_text(cx),
        }
    }
    fn offset(&self, cx: &App) -> Point<Pixels> {
        match self {
            Self::Pdf(pdf) => pdf.read(cx).scroll_offset(),
            Self::Epub(pane) => pane.read(cx).scroll.offset(),
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
#[derive(Clone, Copy, PartialEq)]
struct Viewport {
    bounds: Bounds<Pixels>,
    offset: Point<Pixels>,
    rem: Pixels,
    zoom: f32,
}
pub(crate) struct TranslationOverlay {
    source: Option<Source>,
    capture: Capture,
    pub state: TranslationState,
    generation: u64,
    active: bool,
    viewport: Rc<Cell<Option<Viewport>>>,
    hitbox: Rc<RefCell<Option<Hitbox>>>,
    service: TranslationService,
    subscription: Option<Subscription>,
}
impl TranslationOverlay {
    pub fn new(service: TranslationService) -> Self {
        Self {
            source: None,
            capture: Capture::Idle,
            state: TranslationState::Idle,
            generation: 0,
            active: false,
            viewport: Rc::new(Cell::new(None)),
            hitbox: Rc::new(RefCell::new(None)),
            service,
            subscription: None,
        }
    }
    pub fn reading_area(&self) -> AnyElement {
        let hitbox = self.hitbox.clone();
        canvas(
            move |bounds, window, _| {
                hitbox.replace(Some(window.insert_hitbox(bounds, HitboxBehavior::Normal)));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
    }
    pub fn attach(&mut self, state: &ReaderState, cx: &mut Context<Self>) {
        self.subscription = None;
        self.source = match state {
            ReaderState::Pdf(pdf) => {
                self.subscription = Some(cx.subscribe(pdf, |this, _, event, cx| {
                    if matches!(event, PdfEvent::SelectionChanged)
                        && let Capture::AwaitingPdfText(anchor) = this.capture
                    {
                        this.capture_text(anchor, cx);
                    }
                }));
                Some(Source::Pdf(pdf.clone()))
            }
            ReaderState::Epub(pane) => Some(Source::Epub(pane.clone())),
            _ => None,
        };
        cx.notify();
    }
    pub fn snapshot(&self) -> Option<&SelectionSnapshot> {
        match &self.capture {
            Capture::Offered(snapshot) => Some(snapshot),
            _ => None,
        }
    }
    pub fn invalidate(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.capture = Capture::Idle;
        self.state = TranslationState::Idle;
        cx.notify();
    }
    pub fn deactivate(&mut self, cx: &mut Context<Self>) {
        self.active = false;
        self.invalidate(cx);
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
            "active={} generation={} source={:?} snapshot={:?}",
            self.active,
            self.generation,
            self.source.as_ref().and_then(|s| s.selected_text(cx)),
            self.snapshot()
        )
    }
    fn capture_text(&mut self, anchor: SelectionAnchor, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        if let Some(text) = self
            .source
            .as_ref()
            .and_then(|source| source.selected_text(cx))
        {
            self.capture = Capture::Offered(SelectionSnapshot {
                text,
                anchor,
                generation: self.generation,
            });
            self.state = TranslationState::Armed;
        } else if matches!(&self.source, Some(Source::Pdf(pdf)) if pdf.read(cx).is_selection_pending())
        {
            self.capture = Capture::AwaitingPdfText(anchor);
        } else {
            self.capture = Capture::Idle;
        }
        cx.notify();
    }
    pub fn begin(&mut self, cx: &mut Context<Self>) {
        if !self.active || matches!(self.state, TranslationState::Loading) {
            return;
        }
        if self.snapshot().is_none() {
            if !matches!(self.capture, Capture::Idle) {
                return;
            }
            self.generation += 1;
            self.capture_text(SelectionAnchor::Viewport, cx);
        }
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        let reply = self.service.translate(&snapshot.text);
        self.generation += 1;
        let generation = self.generation;
        self.state = TranslationState::Loading;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = reply
                .await
                .unwrap_or_else(|_| Err("翻译服务已停止，请重试。".into()));
            let _ = this.update(cx, |this, cx| {
                if this.active
                    && this.generation == generation
                    && matches!(this.state, TranslationState::Loading)
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
        if !self.state.is_open() {
            return;
        }
        self.generation += 1;
        self.state = if self.snapshot().is_some() {
            TranslationState::Armed
        } else {
            TranslationState::Idle
        };
        cx.notify();
    }
}

impl Render for TranslationOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let source = self.source.clone();
        let owner = cx.weak_entity();
        let viewport = self.viewport.clone();
        let hitbox = self.hitbox.clone();
        let sample = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let Some(hitbox) = hitbox.borrow().clone() else {
                    return;
                };
                let current = Viewport {
                    bounds,
                    offset: source
                        .as_ref()
                        .map_or(Point::default(), |source| source.offset(cx)),
                    rem: window.rem_size(),
                    zoom: match &source {
                        Some(Source::Pdf(pdf)) => pdf.read(cx).zoom(),
                        _ => 1.,
                    },
                };
                if let Some(old) = viewport.replace(Some(current))
                    && old != current
                {
                    let scrolling = old.bounds == current.bounds
                        && old.rem == current.rem
                        && old.zoom == current.zoom;
                    let owner = owner.clone();
                    let generation = owner.update(cx, |this, _| this.generation).ok();
                    window.defer(cx, move |_, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            if generation == Some(this.generation)
                                && (!scrolling || !matches!(this.capture, Capture::Pressed))
                            {
                                this.invalidate(cx);
                            }
                        });
                    });
                }
                let down_owner = owner.clone();
                let down_hitbox = hitbox.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase.capture()
                        && event.button == MouseButton::Left
                        && down_hitbox.is_hovered(window)
                    {
                        let _ = down_owner.update(cx, |this, cx| {
                            if this.active {
                                this.invalidate(cx);
                                this.capture = Capture::Pressed;
                            }
                        });
                    }
                });
                let scroll_owner = owner.clone();
                window.on_mouse_event(move |_: &ScrollEvent, phase, window, cx| {
                    if phase.capture() && hitbox.should_handle_scroll(window) {
                        let _ = scroll_owner.update(cx, |this, cx| {
                            if this.active && !matches!(this.capture, Capture::Pressed) {
                                this.invalidate(cx);
                            }
                        });
                    }
                });
                let owner = owner.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if !phase.bubble() || event.button != MouseButton::Left {
                        return;
                    }
                    let anchor = SelectionAnchor::Pointer(event.position);
                    let generation = owner
                        .update(cx, |this, _| {
                            (this.active && matches!(this.capture, Capture::Pressed))
                                .then_some(this.generation)
                        })
                        .ok()
                        .flatten();
                    if let Some(generation) = generation {
                        let owner = owner.clone();
                        window.defer(cx, move |window, cx| {
                            window.defer(cx, move |_, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if this.generation == generation
                                        && matches!(this.capture, Capture::Pressed)
                                    {
                                        this.capture_text(anchor, cx);
                                    }
                                });
                            });
                        });
                    }
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
        let Some(snapshot) = self.snapshot() else {
            return root;
        };
        let Some(viewport) = self.viewport.get().map(|v| v.bounds) else {
            return root;
        };
        let size = window.rem_size() * 1.5;
        let gap = window.rem_size() * 0.25;
        let anchor = match snapshot.anchor {
            SelectionAnchor::Pointer(point) => point,
            SelectionAnchor::Viewport => viewport.top_right() - point(size + gap * 2., -gap),
        };
        let left = (anchor.x + gap)
            .min(viewport.right() - size - gap)
            .max(viewport.left() + gap);
        let top = if anchor.y + gap + size <= viewport.bottom() {
            anchor.y + gap
        } else {
            anchor.y - size - gap
        }
        .max(viewport.top() + gap);
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
                crate::controls::observed(content)
            });
        root = root.child(
            div()
                .absolute()
                .left(left - viewport.left())
                .top(top - viewport.top())
                .occlude()
                .child(popover),
        );
        root
    }
}
