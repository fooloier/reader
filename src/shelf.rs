use crate::{app::ReaderApp, controls::icon};
use gpui_kit::{
    component::{
        Size,
        button::{Button, ButtonVariants},
        menu::{DropdownMenu, PopupMenu, PopupMenuItem},
        table::{Column, DataTable, TableDelegate, TableState},
        *,
    },
    prelude::*,
    *,
};
use reader_core::{Book, ShelfLayout};
use std::path::PathBuf;

actions!(
    shelf,
    [OpenSelectedBook, RenameSelectedBook, DeleteSelectedBook]
);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", OpenSelectedBook, Some("Shelf && !Input")),
        KeyBinding::new("f2", RenameSelectedBook, Some("Shelf && !Input")),
        KeyBinding::new("delete", DeleteSelectedBook, Some("Shelf && !Input")),
    ]);
}

pub(crate) struct ShelfTable {
    books: Vec<Book>,
    columns: [Column; 4],
    owner: WeakEntity<ReaderApp>,
    cache: PathBuf,
}

impl ShelfTable {
    pub(crate) fn new(
        books: Vec<Book>,
        owner: WeakEntity<ReaderApp>,
        cache: PathBuf,
        window: &Window,
    ) -> Self {
        let rem = window.rem_size();
        Self {
            books,
            columns: [
                Column::new("cover", "封面").width(rems(6.).to_pixels(rem)),
                Column::new("title", "书名").width(rems(24.).to_pixels(rem)),
                Column::new("authors", "作者").width(rems(16.).to_pixels(rem)),
                Column::new("published", "出版年月").width(rems(10.).to_pixels(rem)),
            ]
            .map(|column| column.min_width(rems(4.).to_pixels(rem)).movable(false)),
            owner,
            cache,
        }
    }

    fn text(&self, row_ix: usize, col_ix: usize) -> String {
        let book = &self.books[row_ix];
        match col_ix {
            0 => format!("《{}》封面", book.display_title),
            1 => book.display_title.clone(),
            2 if book.metadata.authors.is_empty() => "未知".into(),
            2 => book.metadata.authors.join("、"),
            3 => book
                .metadata
                .year
                .map(|year| year.to_string())
                .unwrap_or_else(|| "未知".into()),
            _ => unreachable!(),
        }
    }
}

impl TableDelegate for ShelfTable {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }
    fn rows_count(&self, _: &App) -> usize {
        self.books.len()
    }
    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        div().id(SharedString::from(format!(
            "book-{}",
            self.books[row_ix].id.0
        )))
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .w_full()
            .text_color(cx.theme().foreground)
            .child(self.columns[col_ix].name.clone())
    }

    fn cell_label(&self, row_ix: usize, col_ix: usize, _: &App) -> Option<SharedString> {
        Some(self.text(row_ix, col_ix).into())
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let book = &self.books[row_ix];
        let id = book.id;
        let owner = self.owner.clone();
        match col_ix {
            0 => crate::controls::observed(
                div()
                    .id(SharedString::from(format!("cover-{}", id.0)))
                    .relative()
                    .w_8()
                    .h_12()
                    .flex_shrink_0()
                    .overflow_hidden()
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        let _ = owner.update(cx, |app, cx| app.open_book(id, window, cx));
                    })
                    .child(
                        book.cover
                            .as_ref()
                            .map(|name| {
                                img(self.cache.join(name))
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
                                    .child(Icon::new(IconName::BookOpen).size_6())
                                    .into_any_element()
                            }),
                    ),
            ),
            1 => Button::new(SharedString::from(format!("open-{}", id.0)))
                .ghost()
                .w_full()
                .min_w_0()
                .px_0()
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_left()
                        .text_ellipsis()
                        .child(book.display_title.clone()),
                )
                .accessibility_label(format!("打开《{}》", book.display_title))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    let _ = owner.update(cx, |app, cx| app.open_book(id, window, cx));
                })
                .into_any_element(),
            _ => div()
                .w_full()
                .overflow_hidden()
                .text_ellipsis()
                .child(self.text(row_ix, col_ix))
                .into_any_element(),
        }
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        book_actions(menu, self.books[row_ix].id, &self.owner)
    }
}

fn book_actions(
    menu: PopupMenu,
    id: reader_core::BookId,
    owner: &WeakEntity<ReaderApp>,
) -> PopupMenu {
    let rename_owner = owner.clone();
    let delete_owner = owner.clone();
    menu.item(
        PopupMenuItem::new("重命名…")
            .icon(IconName::FileText)
            .on_click(move |_, window, cx| {
                let _ = rename_owner.update(cx, |app, cx| app.rename_dialog(id, window, cx));
            }),
    )
    .separator()
    .item(
        PopupMenuItem::new("删除…")
            .icon(IconName::Delete)
            .on_click(move |_, window, cx| {
                let _ = delete_owner.update(cx, |app, cx| app.delete_dialog(id, window, cx));
            }),
    )
}

impl ReaderApp {
    pub(crate) fn sync_shelf(&self, cx: &mut Context<Self>) {
        self.shelf_table.update(cx, |table, cx| {
            let selected = table
                .selected_row()
                .and_then(|ix| table.delegate().books.get(ix))
                .map(|book| book.id);
            table.delegate_mut().books.clone_from(&self.books);
            table.clear_selection(cx);
            table.set_right_clicked_row(None, cx);
            if let Some(ix) =
                selected.and_then(|id| self.books.iter().position(|book| book.id == id))
            {
                table.set_selected_row(ix, cx);
            }
            cx.notify();
        });
    }

    fn selected_shelf_book(&self, cx: &App) -> Option<reader_core::BookId> {
        let table = self.shelf_table.read(cx);
        table
            .selected_row()
            .and_then(|ix| table.delegate().books.get(ix))
            .map(|book| book.id)
    }

    pub fn shelf(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
        if self.settings.shelf == ShelfLayout::List {
            return div()
                .image_cache(self.covers.clone())
                .id("shelf")
                .size_full()
                .min_w_0()
                .min_h_0()
                .key_context("Shelf")
                .on_action(cx.listener(|app, _: &OpenSelectedBook, window, cx| {
                    if let Some(id) = app.selected_shelf_book(cx) {
                        app.open_book(id, window, cx);
                    }
                }))
                .on_action(cx.listener(|app, _: &RenameSelectedBook, window, cx| {
                    if let Some(id) = app.selected_shelf_book(cx) {
                        app.rename_dialog(id, window, cx);
                    }
                }))
                .on_action(cx.listener(|app, _: &DeleteSelectedBook, window, cx| {
                    if let Some(id) = app.selected_shelf_book(cx) {
                        app.delete_dialog(id, window, cx);
                    }
                }))
                .child(
                    DataTable::new(&self.shelf_table)
                        .accessibility_label("书架列表")
                        .with_size(Size::Size(rems(4.5).to_pixels(window.rem_size())))
                        .header_row_height(rems(2.5).to_pixels(window.rem_size()))
                        .bordered(false),
                )
                .into_any_element();
        }
        let books = self
            .books
            .iter()
            .map(|book| self.book_card(book, cx).into_any_element())
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
                        .grid()
                        .grid_cols(columns)
                        .max_w(max_width * columns as f32 + gap * (columns - 1) as f32)
                        .children(books),
                )
        })
        .into_any_element()
    }
    fn book_card(&self, book: &Book, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
            .flex_col()
            .child(crate::controls::observed(
                div()
                    .id(SharedString::from(format!("cover-{}", id.0)))
                    .relative()
                    .overflow_hidden()
                    .flex_shrink_0()
                    .cursor_pointer()
                    .h_56()
                    .w_full()
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
                    .child(
                        Button::new(SharedString::from(format!("open-{}", id.0)))
                            .ghost()
                            .min_w_0()
                            .w_full()
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
                            .truncate()
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
                div().flex().justify_end().mt_auto().child(
                    icon(
                        SharedString::from(format!("book-menu-{}", id.0)),
                        IconName::Ellipsis,
                        format!("《{}》的更多操作", book.display_title),
                    )
                    .dropdown_menu_with_anchor(Anchor::TopRight, {
                        let owner = cx.entity().downgrade();
                        move |menu, _, _| book_actions(menu, id, &owner)
                    }),
                ),
            )
    }
}
