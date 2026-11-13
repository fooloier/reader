mod bookmarks;
mod epub;
mod images;
pub(crate) use bookmarks::BookmarkDestination;
pub use epub::EpubPane;
use gpui_kit::{component::ActiveTheme, prelude::*, *};
use gpui_pdf::{PdfEvent, PdfStyle, PdfView};
pub use images::CoverImages;
use reader_core::{Book, Format, Location, PdfSettings, Settings};
use reader_runtime::{AppPaths, epub::EpubBook, service::Service};
use std::{cell::RefCell, rc::Rc, time::Duration};

pub enum ReaderState {
    Loading,
    Failed(String),
    Pdf(Entity<PdfView>),
    Epub(Entity<EpubPane>),
}
pub struct ReadingSession {
    pub book: Book,
    pub state: ReaderState,
    pub position: Option<Location>,
    pub error: Option<String>,
    service: Service,
    subscriptions: Vec<Subscription>,
    save_task: Option<Task<()>>,
    pdf_style: Rc<RefCell<PdfStyle>>,
    closed: bool,
    settings: Settings,
}
impl ReadingSession {
    pub fn new(
        book: Book,
        paths: AppPaths,
        settings: Settings,
        service: Service,
        cx: &mut Context<Self>,
    ) -> Self {
        let position = book.location.clone();
        let pdf_style = Rc::new(RefCell::new(style(cx)));
        let mut session = Self {
            book: book.clone(),
            state: ReaderState::Loading,
            position,
            error: None,
            service: service.clone(),
            subscriptions: vec![],
            save_task: None,
            pdf_style: pdf_style.clone(),
            closed: false,
            settings: settings.clone(),
        };
        let path = paths.books().join(&book.filename);
        match book.format {
            Format::Pdf => {
                let pdf = cx.new(|cx| {
                    PdfView::new(
                        path,
                        Rc::new(move || *pdf_style.borrow()),
                        Rc::new(|| 1.),
                        cx,
                    )
                });
                pdf.update(cx, |pdf, cx| {
                    pdf.set_host_chrome(cx);
                    apply_pdf(pdf, settings.pdf, cx);
                    if let Some(Location::Pdf { page, fraction }) = book.location {
                        pdf.restore_viewport(page, fraction, cx);
                    }
                });
                session
                    .subscriptions
                    .push(cx.subscribe(&pdf, |this, _, event, cx| {
                        if let PdfEvent::ViewportChanged { page, fraction } = event {
                            this.record(
                                Location::Pdf {
                                    page: *page,
                                    fraction: *fraction,
                                },
                                cx,
                            );
                        }
                        cx.notify();
                    }));
                session.state = ReaderState::Pdf(pdf);
            }
            Format::Epub => {
                let job = service.submit(move |_| EpubBook::open(&path));
                cx.spawn(async move |this, cx| {
                    let loaded = job.await;
                    let _ = this.update(cx, |this, cx| {
                        if this.closed {
                            return;
                        }
                        this.state = match loaded {
                            Ok(document) => {
                                let location = this.position.clone();
                                let pane = cx.new(|cx| {
                                    EpubPane::new(
                                        document,
                                        this.settings.epub.clone(),
                                        location,
                                        cx,
                                    )
                                });
                                this.subscriptions.push(
                                    cx.subscribe(&pane, |this, _, location, cx| {
                                        this.record(location.clone(), cx)
                                    }),
                                );
                                ReaderState::Epub(pane)
                            }
                            Err(e) => ReaderState::Failed(e.to_string()),
                        };
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        session
    }
    fn record(&mut self, position: Location, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.position = Some(position.clone());
        let service = self.service.clone();
        let id = self.book.id;
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
            let result = service
                .submit(move |library| library.save_location(id, &position))
                .await;
            if let Err(e) = result {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(format!("无法保存阅读位置：{e}"));
                    cx.notify();
                });
            }
        }));
    }
    pub fn configure(&mut self, settings: &Settings, cx: &mut Context<Self>) {
        self.settings = settings.clone();
        *self.pdf_style.borrow_mut() = style(cx);
        match &self.state {
            ReaderState::Pdf(pdf) => pdf.update(cx, |v, cx| apply_pdf(v, settings.pdf, cx)),
            ReaderState::Epub(pane) => {
                pane.update(cx, |v, cx| v.configure(settings.epub.clone(), cx))
            }
            _ => {}
        }
        cx.notify();
    }
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_task.take();
        if let ReaderState::Epub(pane) = &self.state {
            self.position = Some(pane.read(cx).location());
        }
        if let ReaderState::Pdf(pdf) = &self.state {
            if pdf.read(cx).page_count() > 0 {
                let (page, fraction) = pdf.read(cx).viewport();
                self.position = Some(Location::Pdf { page, fraction });
            }
            pdf.update(cx, |v, cx| v.release(window, cx));
        }
        self.closed = true;
    }
    pub fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if !self.can_step(forward, cx) {
            return;
        }
        match &self.state {
            ReaderState::Pdf(pdf) => pdf.update(cx, |v, cx| {
                if forward {
                    v.next_page(cx)
                } else {
                    v.prev_page(cx)
                }
            }),
            ReaderState::Epub(pane) => pane.update(cx, |v, cx| v.step(forward, cx)),
            _ => {}
        }
    }
    pub(crate) fn can_step(&self, forward: bool, cx: &App) -> bool {
        match &self.state {
            ReaderState::Pdf(pdf) => {
                let pdf = pdf.read(cx);
                if forward {
                    pdf.viewport().0 + 1 < pdf.page_count()
                } else {
                    pdf.viewport().0 > 0
                }
            }
            ReaderState::Epub(pane) => {
                let pane = pane.read(cx);
                if forward {
                    pane.chapter + 1 < pane.document.chapters.len()
                } else {
                    pane.chapter > 0
                }
            }
            _ => false,
        }
    }
}
fn apply_pdf(pdf: &mut PdfView, setting: PdfSettings, cx: &mut Context<PdfView>) {
    match setting {
        PdfSettings::FitWidth => pdf.fit_width(cx),
        PdfSettings::FitPage => pdf.fit_page(cx),
        PdfSettings::Zoom(z) => pdf.set_zoom(z, cx),
    }
}
fn style(cx: &App) -> PdfStyle {
    let theme = cx.theme();
    PdfStyle {
        bg: theme.muted,
        border: theme.border,
        placeholder_bg: theme.background,
        placeholder_fg: theme.muted_foreground,
        header_fg: theme.foreground,
        header_muted: theme.muted_foreground,
    }
}
impl Render for ReadingSession {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.state {
            ReaderState::Loading => div().p_6().child("正在打开书籍…").into_any_element(),
            ReaderState::Failed(e) => div()
                .p_6()
                .child(format!("无法打开书籍：{e}"))
                .into_any_element(),
            ReaderState::Pdf(v) => v.clone().into_any_element(),
            ReaderState::Epub(v) => v.clone().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .children(self.error.clone().map(|e| div().px_4().child(e)))
            .child(div().flex_1().min_h_0().child(content))
    }
}
