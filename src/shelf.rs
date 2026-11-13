use crate::{app::ReaderApp, controls::icon};
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        *,
    },
    prelude::*,
    *,
};
use reader_core::{Book, ShelfLayout};

impl ReaderApp {
    pub fn shelf(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        if self.books.is_empty() {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .child(Icon::new(IconName::BookOpen).size_12())
                .child("书架还是空的")
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("使用工具栏导入 PDF、EPUB 或整个文件夹。"),
                )
                .into_any_element();
        }
        let cards = self.settings.shelf == ShelfLayout::Cards;
        div()
            .image_cache(self.covers.clone())
            .id("shelf")
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .child(
                div()
                    .flex()
                    .gap_4()
                    .when(cards, |view| view.flex_wrap())
                    .when(!cards, |view| view.flex_col())
                    .children(
                        self.books
                            .iter()
                            .map(|book| self.book_card(book, cards, cx)),
                    ),
            )
            .into_any_element()
    }
    fn book_card(
        &self,
        book: &Book,
        cards: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = book.id;
        let cover = book
            .cover
            .as_ref()
            .map(|name| {
                img(self.paths.cache.join(name))
                    .object_fit(ObjectFit::Contain)
                    .absolute()
                    .size_full()
                    .into_any_element()
            })
            .unwrap_or_else(|| {
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(cx.theme().muted)
                    .child(Icon::new(IconName::BookOpen).size_12())
                    .into_any_element()
            });
        let metadata = format!(
            "{} · {}",
            if book.metadata.authors.is_empty() {
                "作者未知".into()
            } else {
                book.metadata.authors.join("、")
            },
            book.metadata
                .year
                .map(|y| y.to_string())
                .unwrap_or_else(|| "出版年份未知".into())
        );
        div()
            .id(SharedString::from(format!("book-{}", id.0)))
            .flex()
            .gap_3()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .when(cards, |view| view.flex_col().w_56())
            .when(!cards, |view| view.items_center().w_full())
            .child(crate::controls::observed(
                div()
                    .id(SharedString::from(format!("cover-{}", id.0)))
                    .relative()
                    .overflow_hidden()
                    .flex_shrink_0()
                    .cursor_pointer()
                    .when(cards, |view| view.h_56().w_full())
                    .when(!cards, |view| view.h_16().w_12())
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.open_book(id, window, cx)),
                    )
                    .child(cover),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .min_w_0()
                    .when(!cards, |view| view.flex_1())
                    .child(
                        Button::new(SharedString::from(format!("open-{}", id.0)))
                            .ghost()
                            .label(book.display_title.clone())
                            .accessibility_label(format!("打开《{}》", book.display_title))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_book(id, window, cx)
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(metadata),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(book.format.extension().to_uppercase()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        icon(
                            SharedString::from(format!("rename-{}", id.0)),
                            IconName::FileText,
                            format!("重命名《{}》…", book.display_title),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.rename_dialog(id, window, cx),
                        )),
                    )
                    .child(
                        icon(
                            SharedString::from(format!("delete-{}", id.0)),
                            IconName::Delete,
                            format!("删除《{}》…", book.display_title),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.delete_dialog(id, window, cx),
                        )),
                    ),
            )
    }
}
