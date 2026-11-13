#[cfg(feature = "test-support")]
use crate::reading::ReaderState;
use crate::{
    controls::icon,
    reading::{CoverImages, ReadingSession},
};
use anyhow::Result;
use gpui_kit::{
    component::{
        tab::{Tab, TabBar},
        *,
    },
    prelude::*,
    *,
};
use reader_core::{Appearance, Book, BookId, Settings, Workspace};
use reader_runtime::{AppPaths, Library, service::Service};
use std::{collections::HashMap, path::PathBuf};

actions!(
    reader,
    [ImportFiles, ImportFolder, OpenSettings, CloseBook, Quit]
);

pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    let keys = if cfg!(target_os = "macos") {
        ["cmd-o", "cmd-shift-o", "cmd-,", "cmd-w", "cmd-q"]
    } else {
        ["ctrl-o", "ctrl-shift-o", "ctrl-,", "ctrl-w", "ctrl-q"]
    };
    cx.bind_keys([
        KeyBinding::new(keys[0], ImportFiles, Some("Reader")),
        KeyBinding::new(keys[1], ImportFolder, Some("Reader")),
        KeyBinding::new(keys[2], OpenSettings, Some("Reader")),
        KeyBinding::new(keys[3], CloseBook, Some("Reader")),
        KeyBinding::new(keys[4], Quit, Some("Reader")),
    ]);
}

pub struct Boot {
    pub books: Vec<Book>,
    pub settings: Settings,
    pub workspace: Workspace,
    pub paths: AppPaths,
    pub service: Service,
    pub diagnostics: Vec<String>,
}
impl Boot {
    pub fn load(library: Library) -> Result<Self> {
        Ok(Self {
            books: library.books()?,
            settings: library.settings()?,
            workspace: library.workspace()?,
            diagnostics: library.diagnostics(),
            paths: library.paths.clone(),
            service: Service::new(library),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shutdown {
    Running,
    Saving,
    Saved,
}

pub struct ReaderApp {
    pub books: Vec<Book>,
    pub settings: Settings,
    pub workspace: Workspace,
    pub sessions: HashMap<BookId, Entity<ReadingSession>>,
    pub status: String,
    pub busy: bool,
    pub paths: AppPaths,
    pub service: Service,
    focus: FocusHandle,
    pub(crate) covers: Entity<CoverImages>,
    pub(crate) settings_focus: FocusHandle,
    pub(crate) toc_focus: FocusHandle,
    pub(crate) shutdown: Shutdown,
    pub(crate) sheet_subscription: Option<Subscription>,
    pub overlay: Option<&'static str>,
}

impl ReaderApp {
    #[cfg(feature = "test-support")]
    pub fn is_ready(&self, cx: &App) -> bool {
        self.workspace
            .active
            .and_then(|id| self.sessions.get(&id))
            .is_none_or(|session| match &session.read(cx).state {
                ReaderState::Epub(_) => true,
                ReaderState::Pdf(pdf) => pdf.read(cx).is_viewport_ready(),
                _ => false,
            })
    }

    pub fn new(mut boot: Boot, window: &mut Window, cx: &mut Context<Self>) -> Self {
        if boot.settings.epub.font != ".SystemUIFont"
            && !cx
                .text_system()
                .all_font_names()
                .contains(&boot.settings.epub.font)
        {
            boot.settings.epub.font = ".SystemUIFont".into();
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Theme::change(
            match boot.settings.appearance {
                Appearance::Light => ThemeMode::Light,
                Appearance::Dark => ThemeMode::Dark,
            },
            Some(window),
            cx,
        );
        let mut this = Self {
            books: boot.books,
            settings: boot.settings,
            workspace: boot.workspace,
            sessions: HashMap::new(),
            status: boot.diagnostics.join("；"),
            busy: false,
            paths: boot.paths,
            service: boot.service,
            focus,
            covers: CoverImages::new(cx),
            settings_focus: cx.focus_handle().tab_stop(true),
            toc_focus: cx.focus_handle().tab_stop(true),
            shutdown: Shutdown::Running,
            sheet_subscription: None,
            overlay: None,
        };
        for id in this.workspace.tabs.clone() {
            this.create_session(id, cx);
        }
        let owner = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = owner.update(cx, |this, cx| this.begin_shutdown(window, cx));
            false
        });
        this
    }
    pub(crate) fn create_session(&mut self, id: BookId, cx: &mut Context<Self>) {
        if self.sessions.contains_key(&id) {
            return;
        }
        if let Some(book) = self.books.iter().find(|b| b.id == id).cloned() {
            let session = cx.new(|cx| {
                ReadingSession::new(
                    book,
                    self.paths.clone(),
                    self.settings.clone(),
                    self.service.clone(),
                    cx,
                )
            });
            self.sessions.insert(id, session);
        }
    }
    pub fn open_book(&mut self, id: BookId, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.create_session(id, cx);
        self.workspace.open(id);
        self.persist_workspace(cx);
        cx.notify();
    }
    pub fn close_book(&mut self, id: BookId, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.retire_session(id, window, cx);
        self.workspace.close(id);
        self.persist_workspace(cx);
        cx.notify();
    }
    pub(crate) fn retire_session(
        &mut self,
        id: BookId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.sessions.remove(&id) {
            session.update(cx, |v, cx| v.close(window, cx));
            let position = session.read(cx).position.clone();
            if let Some(book) = self.books.iter_mut().find(|book| book.id == id) {
                book.location = position.clone();
            }
            if let Some(position) = position {
                let job = self
                    .service
                    .submit(move |library| library.save_location(id, &position));
                self.report(job, cx);
            }
        }
    }
    pub(crate) fn begin_shutdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shutdown != Shutdown::Running {
            return;
        }
        self.shutdown = Shutdown::Saving;
        window.close_dialog(cx);
        window.close_sheet(cx);
        for id in self.sessions.keys().copied().collect::<Vec<_>>() {
            self.retire_session(id, window, cx);
        }
        let positions = self
            .books
            .iter()
            .filter_map(|book| book.location.clone().map(|p| (book.id, p)))
            .collect::<Vec<_>>();
        let workspace = self.workspace.clone();
        let settings = self.settings.clone();
        let job = self.service.submit(move |library| {
            let existing = library.books()?;
            for (id, position) in positions {
                if existing.iter().any(|b| b.id == id) {
                    library.save_location(id, &position)?;
                }
            }
            library.save_settings(&settings)?;
            library.save_workspace(&workspace)
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.shutdown = Shutdown::Saved;
                        cx.quit();
                    }
                    Err(error) => {
                        this.shutdown = Shutdown::Running;
                        this.status = format!("退出保存失败：{error}。请检查数据目录后重试退出。");
                        for id in this.workspace.tabs.clone() {
                            this.create_session(id, cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn persist_workspace(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        let job = self
            .service
            .submit(move |library| library.save_workspace(&workspace));
        self.report(job, cx);
    }
    pub fn report(
        &mut self,
        job: impl Future<Output = Result<()>> + 'static,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            if let Err(e) = job.await {
                let _ = this.update(cx, |this, cx| {
                    this.status = format!("操作失败：{e}");
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub fn update_settings(
        &mut self,
        settings: Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings = settings;
        Theme::change(
            match self.settings.appearance {
                Appearance::Light => ThemeMode::Light,
                Appearance::Dark => ThemeMode::Dark,
            },
            Some(window),
            cx,
        );
        for session in self.sessions.values() {
            session.update(cx, |v, cx| v.configure(&self.settings, cx));
        }
        let settings = self.settings.clone();
        let job = self
            .service
            .submit(move |library| library.save_settings(&settings));
        self.report(job, cx);
        cx.notify();
    }
    pub(crate) fn choose_import(&mut self, folder: bool, cx: &mut Context<Self>) {
        if self.busy || self.shutdown != Shutdown::Running {
            return;
        }
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: true,
            prompt: Some(
                if folder {
                    "导入文件夹"
                } else {
                    "导入 PDF 或 EPUB"
                }
                .into(),
            ),
        });
        cx.spawn(async move |this, cx| match prompt.await {
            Ok(Ok(Some(paths))) => {
                let _ = this.update(cx, |this, cx| this.import(paths, cx));
            }
            Ok(Ok(None)) => {}
            error => {
                let _ = this.update(cx, |this, cx| {
                    this.status = format!("无法打开文件选择器：{error:?}");
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub fn import(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.busy || self.shutdown != Shutdown::Running {
            return;
        }
        self.busy = true;
        self.status = "正在导入书籍…".into();
        cx.notify();
        let job = self.service.submit(move |library| {
            let results = library.import(&paths);
            Ok((results, library.books()?))
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((results, books)) => {
                        this.books = books;
                        let imported = results.iter().filter(|r| r.status == "imported").count();
                        let duplicates = results.iter().filter(|r| r.status == "duplicate").count();
                        let skipped = results.iter().filter(|r| r.status == "skipped").count();
                        let failures = results
                            .iter()
                            .filter(|r| r.status == "failed")
                            .filter_map(|r| {
                                r.error
                                    .as_ref()
                                    .map(|e| format!("{}：{e}", r.path.display()))
                            })
                            .collect::<Vec<_>>();
                        this.status = format!(
                            "已导入 {imported} 本，跳过 {duplicates} 本重复书籍及 {skipped} 项非书籍或嵌套链接，失败 {} 项。{}",
                            failures.len(),
                            failures.join("；")
                        );
                    }
                    Err(e) => this.status = format!("导入失败：{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
impl Render for ReaderApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.shutdown != Shutdown::Running {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .child("正在保存阅读位置和设置，请稍候…")
                .into_any_element();
        }
        let selected = self
            .workspace
            .active
            .and_then(|id| self.workspace.tabs.iter().position(|b| *b == id))
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut tabs = vec![Tab::new().label("书架").aria_label("书架")];
        for id in &self.workspace.tabs {
            if let Some(book) = self.books.iter().find(|b| b.id == *id) {
                let id = *id;
                tabs.push(
                    Tab::new().label(book.display_title.clone()).suffix(
                        icon(
                            SharedString::from(format!("close-{}", id.0)),
                            IconName::Close,
                            format!("关闭《{}》", book.display_title),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.close_book(id, window, cx);
                            },
                        )),
                    ),
                );
            }
        }
        let content =
            if let Some(session) = self.workspace.active.and_then(|id| self.sessions.get(&id)) {
                session.clone().into_any_element()
            } else {
                self.shelf(cx).into_any_element()
            };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus)
            .key_context("Reader")
            .on_action(cx.listener(|this, _: &ImportFiles, _, cx| this.choose_import(false, cx)))
            .on_action(cx.listener(|this, _: &ImportFolder, _, cx| this.choose_import(true, cx)))
            .on_action(
                cx.listener(|this, _: &OpenSettings, window, cx| this.settings_sheet(window, cx)),
            )
            .on_action(cx.listener(|this, _: &CloseBook, window, cx| {
                if let Some(id) = this.workspace.active {
                    this.close_book(id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Quit, window, cx| this.begin_shutdown(window, cx)))
            .child(
                TabBar::new("book-tabs")
                    .selected_index(selected)
                    .children(tabs)
                    .on_click(cx.listener(|this, index, window, cx| {
                        window.focus(&this.focus, cx);
                        this.workspace.active = if *index == 0 {
                            None
                        } else {
                            this.workspace.tabs.get(index - 1).copied()
                        };
                        this.persist_workspace(cx);
                        cx.notify();
                    })),
            )
            .child(self.toolbar(cx))
            .child(div().flex_1().min_h_0().child(content))
            .when(!self.status.is_empty(), |view| {
                view.child(
                    div()
                        .px_4()
                        .py_2()
                        .text_sm()
                        .max_h_24()
                        .overflow_hidden()
                        .child(self.status.clone()),
                )
            })
            .into_any_element()
    }
}
