use crate::{app::ReaderApp, controls::icon};
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        menu::{DropdownMenu, PopupMenuItem},
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
        let books = self
            .books
            .iter()
            .map(|book| self.book_card(book, cards, cx).into_any_element())
            .collect::<Vec<_>>();
        let covers = self.covers.clone();
        container_query(move |size, window, _| {
            let gap = rems(1.).to_pixels(window.rem_size());
            let min_width = rems(12.).to_pixels(window.rem_size());
            let max_width = rems(20.).to_pixels(window.rem_size());
            let width = (size.width - gap * 2.).max(Pixels::ZERO);
            let columns = ((width + gap) / (min_width + gap)).floor().max(1.) as u16;
            div()
                .image_cache(covers)
                .id("shelf")
                .size_full()
                .overflow_y_scroll()
                .p_4()
                .child(
                    div()
                        .w_full()
                        .gap_4()
                        .when(cards, |view| {
                            view.grid()
                                .grid_cols(columns)
                                .max_w(max_width * columns as f32 + gap * (columns - 1) as f32)
                        })
                        .when(!cards, |view| view.flex().flex_col())
                        .children(books),
                )
        })
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
            .min_w_0()
            .w_full()
            .gap_3()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .when(cards, |view| view.flex_col())
            .when(!cards, |view| view.items_center())
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
                            .min_w_0()
                            .when(cards, |view| view.w_full())
                            .label(book.display_title.clone())
                            .tooltip(book.display_title.clone())
                            .accessibility_label(format!("打开《{}》", book.display_title))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_book(id, window, cx)
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .when(cards, |view| view.truncate())
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
            .when(cards, |view| {
                let app = cx.entity().downgrade();
                view.child(
                    div().flex().justify_end().mt_auto().child(
                        icon(
                            SharedString::from(format!("book-menu-{}", id.0)),
                            IconName::Ellipsis,
                            format!("《{}》的更多操作", book.display_title),
                        )
                        .dropdown_menu_with_anchor(
                            Anchor::TopRight,
                            move |menu, _, _| {
                                let rename_app = app.clone();
                                let delete_app = app.clone();
                                menu.item(
                                    PopupMenuItem::new("重命名")
                                        .icon(IconName::FileText)
                                        .on_click(move |_, window, cx| {
                                            let _ = rename_app.update(cx, |app, cx| {
                                                app.rename_dialog(id, window, cx);
                                            });
                                        }),
                                )
                                .separator()
                                .item(
                                    PopupMenuItem::new("删除").icon(IconName::Delete).on_click(
                                        move |_, window, cx| {
                                            let _ = delete_app.update(cx, |app, cx| {
                                                app.delete_dialog(id, window, cx);
                                            });
                                        },
                                    ),
                                )
                            },
                        ),
                    ),
                )
            })
            .when(!cards, |view| {
                view.child(
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
            })
    }
}
