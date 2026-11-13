use crate::{app::ReaderApp, reading::ReaderState, settings::SettingsPane};
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        dialog::{DialogAction, DialogButtonProps, DialogFooter},
        input::{Input, InputState},
        *,
    },
    prelude::*,
    *,
};
use reader_core::BookId;

impl ReaderApp {
    pub fn rename_dialog(&mut self, id: BookId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = self.books.iter().find(|b| b.id == id) else {
            return;
        };
        let title = book.display_title.clone();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title.clone()));
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            let input_value = input.clone();
            dialog
                .title(format!("重命名《{title}》"))
                .child("书名")
                .child(Input::new(&input).id("rename-title"))
                .child(
                    div()
                        .text_sm()
                        .child("书名须为 1 到 200 个字符。原始元数据保持不变。"),
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("cancel")
                                .label("取消")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            DialogAction::new().child(Button::new("ok").primary().label("保存")),
                        ),
                )
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("保存")
                        .cancel_text("取消")
                        .show_cancel(true),
                )
                .on_ok(move |_, window, cx| {
                    let title = input_value.read(cx).value().to_string();
                    if title.trim().is_empty() || title.chars().count() > 200 {
                        return false;
                    }
                    let _ = owner.update(cx, |this, cx| {
                        let was_open = this.sessions.contains_key(&id);
                        this.retire_session(id, window, cx);
                        let job = this
                            .service
                            .submit(move |library| library.rename(id, &title));
                        cx.spawn(async move |this, cx| {
                            let result = job.await;
                            let _ = this.update(cx, |this, cx| {
                                match result {
                                    Ok(book) => {
                                        if let Some(old) =
                                            this.books.iter_mut().find(|b| b.id == id)
                                        {
                                            *old = book;
                                        }
                                    }
                                    Err(e) => this.status = format!("重命名失败：{e}"),
                                }
                                if was_open
                                    && this.workspace.tabs.contains(&id)
                                    && this.shutdown == crate::app::Shutdown::Running
                                {
                                    this.create_session(id, cx);
                                }
                                cx.notify();
                            });
                        })
                        .detach();
                    });
                    true
                })
        });
    }
    pub fn delete_dialog(&mut self, id: BookId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = self.books.iter().find(|b| b.id == id) else {
            return;
        };
        let title = book.display_title.clone();
        let owner = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            dialog
                .title(format!("删除《{title}》？"))
                .description("将移除书库中的托管副本和阅读进度。原始文件不会被修改或删除。")
                .button_props(
                    DialogButtonProps::default().ok_variant(button::ButtonVariant::Danger),
                )
                .confirm()
                .ok_text("删除")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.close_book(id, window, cx);
                        let job = this.service.submit(move |library| {
                            library.delete(id)?;
                            library.books()
                        });
                        cx.spawn(async move |this, cx| {
                            let result = job.await;
                            let _ = this.update(cx, |this, cx| {
                                match result {
                                    Ok(books) => this.books = books,
                                    Err(e) => this.status = format!("删除失败：{e}"),
                                }
                                cx.notify();
                            });
                        })
                        .detach();
                    });
                    true
                })
        });
    }
    pub(crate) fn settings_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.close_sheet(cx);
        window.focus(&self.settings_focus, cx);
        self.overlay = Some("settings");
        let format = self
            .workspace
            .active
            .and_then(|id| self.books.iter().find(|b| b.id == id).map(|b| b.format));
        let pane = cx.new(|cx| SettingsPane::new(self.settings.clone(), format, window, cx));
        self.sheet_subscription = Some(cx.subscribe_in(
            &pane,
            window,
            |this, _, settings, window, cx| this.update_settings(settings.clone(), window, cx),
        ));
        let owner = cx.weak_entity();
        window.open_sheet_at(Placement::Right, cx, move |sheet, _, _| {
            let owner = owner.clone();
            sheet
                .title("设置")
                .size(rems(24.))
                .child(pane.clone())
                .on_close(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.overlay = None;
                        cx.notify();
                    });
                })
        });
        cx.notify();
    }
    pub(crate) fn toc_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.close_sheet(cx);
        window.focus(&self.toc_focus, cx);
        self.overlay = Some("toc");
        self.sheet_subscription = None;
        let session = self
            .workspace
            .active
            .and_then(|id| self.sessions.get(&id))
            .cloned();
        let owner = cx.weak_entity();
        window.open_sheet_at(Placement::Left, cx, move |sheet, _, cx| {
            let mut content = div().flex().flex_col().gap_2();
            let mut count = 0;
            if let Some(session) = &session {
                match &session.read(cx).state {
                    ReaderState::Pdf(pdf) => {
                        for (i, item) in pdf.read(cx).outline().iter().enumerate() {
                            count += 1;
                            let pdf = pdf.clone();
                            let page = item.page;
                            let owner = owner.clone();
                            content = content.child(
                                Button::new(("toc-pdf", i))
                                    .ghost()
                                    .label(item.title.clone())
                                    .ml(rems(item.level.min(16) as f32))
                                    .disabled(page.is_none())
                                    .on_click(move |_, window, cx| {
                                        if let Some(page) = page {
                                            pdf.update(cx, |v, cx| v.go_to_page(page, cx));
                                        }
                                        window.close_sheet(cx);
                                        let _ = owner.update(cx, |this, cx| {
                                            this.overlay = None;
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                    }
                    ReaderState::Epub(pane) => {
                        for (i, item) in pane.read(cx).document.toc.iter().enumerate() {
                            count += 1;
                            let pane = pane.clone();
                            let resolved = pane.read(cx).document.destination(&item.href).is_some();
                            let depth = item.depth;
                            let href = item.href.clone();
                            let owner = owner.clone();
                            content = content.child(
                                Button::new(("toc-epub", i))
                                    .ghost()
                                    .label(item.title.clone())
                                    .ml(rems(depth.min(16) as f32))
                                    .disabled(!resolved)
                                    .on_click(move |_, window, cx| {
                                        pane.update(cx, |v, cx| v.navigate(&href, cx));
                                        window.close_sheet(cx);
                                        let _ = owner.update(cx, |this, cx| {
                                            this.overlay = None;
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                    }
                    _ => {}
                }
            }
            let owner = owner.clone();
            sheet
                .title("目录与书签")
                .size(rems(22.))
                .child(content)
                .when(count == 0, |sheet| sheet.child("这本书没有可显示的目录"))
                .child(
                    div()
                        .mt_4()
                        .text_sm()
                        .child("书签来自书内目录；本版本不创建个人书签。"),
                )
                .on_close(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.overlay = None;
                        cx.notify();
                    });
                })
        });
        cx.notify();
    }
}
