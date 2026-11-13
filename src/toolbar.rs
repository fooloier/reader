use crate::{app::ReaderApp, controls::icon, reading::ReaderState};
use gpui_kit::{component::*, prelude::*, *};
use reader_core::{Appearance, PdfSettings, ShelfLayout};

impl ReaderApp {
    pub(crate) fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let reading = self.workspace.active.is_some();
        let pdf = self
            .workspace
            .active
            .and_then(|id| self.sessions.get(&id))
            .and_then(|session| match &session.read(cx).state {
                ReaderState::Pdf(pdf) => Some(pdf.clone()),
                _ => None,
            });
        let (previous, next) = self
            .workspace
            .active
            .and_then(|id| self.sessions.get(&id))
            .map(|session| match &session.read(cx).state {
                ReaderState::Pdf(pdf) => {
                    let pdf = pdf.read(cx);
                    (
                        pdf.viewport().0 > 0,
                        pdf.viewport().0 + 1 < pdf.page_count(),
                    )
                }
                ReaderState::Epub(pane) => {
                    let pane = pane.read(cx);
                    (
                        pane.chapter > 0,
                        pane.chapter + 1 < pane.document.chapters.len(),
                    )
                }
                _ => (false, false),
            })
            .unwrap_or((false, false));
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                icon("import-files", IconName::File, "导入文件…")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_import(false, cx))),
            )
            .child(
                icon("import-folder", IconName::FolderOpen, "导入文件夹…")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_import(true, cx))),
            )
            .when(!reading, |row| {
                row.child(
                    icon(
                        "shelf-layout",
                        if self.settings.shelf == ShelfLayout::Cards {
                            IconName::Menu
                        } else {
                            IconName::LayoutDashboard
                        },
                        if self.settings.shelf == ShelfLayout::Cards {
                            "切换到列表"
                        } else {
                            "切换到卡片"
                        },
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        let mut settings = this.settings.clone();
                        settings.shelf = if settings.shelf == ShelfLayout::Cards {
                            ShelfLayout::List
                        } else {
                            ShelfLayout::Cards
                        };
                        this.update_settings(settings, window, cx);
                    })),
                )
            })
            .when(reading, |row| {
                row.child(
                    icon("toc", IconName::PanelLeft, "目录与书签…")
                        .track_focus(&self.toc_focus)
                        .toggled(self.overlay == Some("toc"))
                        .on_click(cx.listener(|this, _, window, cx| this.toc_sheet(window, cx))),
                )
                .child(
                    icon("previous", IconName::ChevronLeft, "上一页或上一章")
                        .disabled(!previous)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(v) =
                                this.workspace.active.and_then(|id| this.sessions.get(&id))
                            {
                                v.update(cx, |v, cx| v.step(false, cx));
                            }
                        })),
                )
                .child(
                    icon("next", IconName::ChevronRight, "下一页或下一章")
                        .disabled(!next)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(v) =
                                this.workspace.active.and_then(|id| this.sessions.get(&id))
                            {
                                v.update(cx, |v, cx| v.step(true, cx));
                            }
                        })),
                )
            })
            .when_some(pdf, |row, pdf| {
                let zoom = pdf.read(cx).zoom();
                let page = pdf.read(cx).viewport().0 + 1;
                let count = pdf.read(cx).page_count();
                row.child(div().text_sm().child(format!("{page} / {count}")))
                    .child(
                        icon("zoom-out", IconName::Minus, "缩小 PDF")
                            .disabled(zoom <= 0.25)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let mut s = this.settings.clone();
                                s.pdf = PdfSettings::Zoom((zoom - 0.25).max(0.25));
                                this.update_settings(s, window, cx);
                            })),
                    )
                    .child(div().text_sm().child(format!("{:.0}%", zoom * 100.)))
                    .child(
                        icon("zoom-in", IconName::Plus, "放大 PDF")
                            .disabled(zoom >= 4.)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let mut s = this.settings.clone();
                                s.pdf = PdfSettings::Zoom((zoom + 0.25).min(4.));
                                this.update_settings(s, window, cx);
                            })),
                    )
                    .child(
                        icon("toolbar-fit-width", IconName::Frame, "适合宽度").on_click(
                            cx.listener(|this, _, window, cx| {
                                let mut s = this.settings.clone();
                                s.pdf = PdfSettings::FitWidth;
                                this.update_settings(s, window, cx);
                            }),
                        ),
                    )
                    .child(
                        icon("toolbar-fit-page", IconName::Maximize, "适合整页").on_click(
                            cx.listener(|this, _, window, cx| {
                                let mut s = this.settings.clone();
                                s.pdf = PdfSettings::FitPage;
                                this.update_settings(s, window, cx);
                            }),
                        ),
                    )
            })
            .child(div().flex_1())
            .child(
                icon(
                    "theme",
                    if self.settings.appearance == Appearance::Light {
                        IconName::Moon
                    } else {
                        IconName::Sun
                    },
                    "切换明暗主题",
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    let mut s = this.settings.clone();
                    s.appearance = if s.appearance == Appearance::Light {
                        Appearance::Dark
                    } else {
                        Appearance::Light
                    };
                    this.update_settings(s, window, cx);
                })),
            )
            .child(
                icon("settings", IconName::Settings, "设置…")
                    .track_focus(&self.settings_focus)
                    .toggled(self.overlay == Some("settings"))
                    .on_click(cx.listener(|this, _, window, cx| this.settings_sheet(window, cx))),
            )
    }
}
