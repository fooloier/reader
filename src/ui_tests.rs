use crate::app::{Boot, ReaderApp};
use crate::reading::ReaderState;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Entity, TestAppContext, TestDispatcher, Window,
    WindowBounds, WindowOptions, point, px, size,
};
use reader_core::{Appearance, BookId, ShelfLayout};
use reader_runtime::{AppPaths, Library};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Harness {
    cx: TestAppContext,
    window: AnyWindowHandle,
    app: Entity<ReaderApp>,
}
impl Harness {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(
            std::env::var("READER_UI_ROOT").expect("请通过 python3 tests/run.py ui 运行"),
        );
        let paths = AppPaths::isolated(&root.join(name)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let library = loop {
            match Library::open(paths.clone()) {
                Ok(library) => break library,
                Err(error)
                    if error.to_string().contains("另一个 Reader") && Instant::now() < deadline =>
                {
                    std::thread::yield_now()
                }
                Err(error) => panic!("无法启动隔离书库：{error:#}"),
            }
        };
        let boot = Boot::load(library).unwrap();
        let platform = gpui_kit::platform::current_platform(true);
        let cx = TestAppContext::build_with_text_system(
            TestDispatcher::new(0),
            Some("reader_ui"),
            platform.text_system(),
        );
        cx.update(|cx| {
            crate::app::init(cx);
            cx.set_reduce_motion(true);
        });
        let (window, app) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(1100.), px(800.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| ReaderApp::new(boot, window, cx)),
            )
            .unwrap()
        });
        let mut harness = Self { cx, window, app };
        harness.frame(|_, _| {});
        harness
    }
    fn frame(&mut self, f: impl FnOnce(&mut Window, &mut App)) {
        self.cx
            .update_window(self.window, |_, window, cx| {
                window.simulate_next_frame(cx);
                window.render_frame(cx);
                f(window, cx);
            })
            .unwrap();
        self.cx.run_until_parked();
    }
    fn click(&mut self, id: &str) {
        self.frame(|window, cx| window.click(gpui_kit::SharedString::from(id.to_owned()), cx));
    }
    fn click_label(&mut self, label: &str) {
        self.frame(|window, cx| {
            let target = window.find_by_label(label).bounds().center();
            use gpui_kit::test::TestEventExt;
            window.pointer_down(target, gpui_kit::MouseButton::Left, cx);
            window.pointer_up(target, gpui_kit::MouseButton::Left, cx);
        });
    }
    fn press(&mut self, key: &str) {
        self.frame(|window, cx| window.press(key, cx));
    }
    fn reading_index(&self, id: BookId) -> usize {
        self.cx
            .read(|cx| match &self.app.read(cx).sessions[&id].read(cx).state {
                ReaderState::Pdf(pdf) => pdf.read(cx).viewport().0,
                ReaderState::Epub(pane) => pane.read(cx).chapter,
                _ => panic!("阅读会话未就绪"),
            })
    }
    #[track_caller]
    fn wait(&mut self, predicate: impl Fn(&ReaderApp, &App) -> bool) {
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            self.frame(|_, _| {});
            if self.cx.read(|cx| predicate(self.app.read(cx), cx)) {
                break;
            }
            assert!(
                Instant::now() < until,
                "界面状态超时：{}",
                self.cx.read(|cx| {
                    let app = self.app.read(cx);
                    format!(
                        "status={} books={} tabs={:?} positions={:?}",
                        app.status,
                        app.books.len(),
                        app.workspace.tabs,
                        app.sessions
                            .values()
                            .map(|v| v.read(cx).position.clone())
                            .collect::<Vec<_>>()
                    )
                })
            );
            std::thread::yield_now();
        }
    }
    fn import(&mut self, filename: &str) -> BookId {
        self.click("import-files");
        assert!(self.cx.did_prompt_for_paths());
        let root = PathBuf::from(std::env::var("READER_UI_ROOT").unwrap());
        self.cx.simulate_path_prompt_response(|options| {
            assert!(options.files);
            Some(vec![root.join(filename)])
        });
        self.wait(|app, _| !app.busy && !app.books.is_empty());
        self.cx.read(|cx| self.app.read(cx).books[0].id)
    }
}

#[test]
fn production_shelf_cards_reflow_with_consistent_columns() {
    let mut h = Harness::new("responsive-shelf");
    h.click("import-folder");
    let root = PathBuf::from(std::env::var("READER_UI_ROOT").unwrap());
    h.cx.simulate_path_prompt_response(|_| Some(vec![root.join("responsive-shelf")]));
    h.wait(|app, _| !app.busy && app.books.len() == 7);
    let ids = h.cx.read(|cx| {
        h.app
            .read(cx)
            .books
            .iter()
            .map(|book| book.id)
            .collect::<Vec<_>>()
    });
    for font_size in [16., 20.] {
        h.cx.update(|cx| {
            gpui_kit::component::Theme::update(cx, |theme| theme.font_size = px(font_size))
        });
        let mut previous_columns = 0;
        for width in [
            160., 320., 431., 432., 433., 539., 540., 541., 620., 900., 1100., 1500.,
        ] {
            h.cx.simulate_window_resize(h.window, size(px(width), px(3500.)));
            h.frame(|window, _| {
                let covers = ids
                    .iter()
                    .map(|id| {
                        window
                            .find(gpui_kit::SharedString::from(format!("cover-{}", id.0)))
                            .bounds()
                    })
                    .collect::<Vec<_>>();
                let columns = covers
                    .iter()
                    .take_while(|bounds| bounds.top() == covers[0].top())
                    .count();
                assert!(columns >= previous_columns);
                previous_columns = columns;
                let rem = window.rem_size();
                let inset = rem * 1.75 + px(1.);
                let card_width = covers[0].size.width + rem * 1.5 + px(2.);
                assert!(card_width <= rem * 20. + px(1.));
                assert!(card_width >= rem * 12. - px(1.) || columns == 1);
                assert!((covers[0].left() - inset).abs() <= px(1.));
                if columns > 1 {
                    assert!(
                        (covers[columns - 1].right() - (px(width) - inset)).abs() <= px(1.),
                        "完整行应填满可用宽度，窗口宽度 {width}"
                    );
                }
                for (ix, bounds) in covers.iter().enumerate() {
                    assert!(
                        (bounds.size.width - covers[0].size.width).abs() <= px(1.),
                        "末行应保持相同列宽"
                    );
                    assert!((bounds.left() - covers[ix % columns].left()).abs() <= px(1.));
                    assert!(bounds.right() <= px(width) - inset + px(1.));
                    let title = window
                        .find(gpui_kit::SharedString::from(format!("open-{}", ids[ix].0)))
                        .bounds();
                    assert!(
                        title.left() >= bounds.left() && title.right() <= bounds.right() + px(1.),
                        "长书名不应撑开卡片"
                    );
                    if ix % columns > 0 {
                        assert!(
                            (bounds.left() - covers[ix - 1].right() - (rem * 2.5 + px(2.))).abs()
                                <= px(1.)
                        );
                    }
                }
            });
        }
        assert!(previous_columns >= 4);
    }
    h.click("shelf-layout");
    h.frame(|window, _| {
        let covers = ids
            .iter()
            .map(|id| {
                window
                    .find(gpui_kit::SharedString::from(format!("cover-{}", id.0)))
                    .bounds()
            })
            .collect::<Vec<_>>();
        assert!(
            covers
                .windows(2)
                .all(|pair| pair[0].left() == pair[1].left() && pair[0].bottom() < pair[1].top())
        );
    });
    h.click("shelf-layout");
    h.click(&format!("open-{}", ids[0].0));
    h.wait(|app, cx| matches!(&app.sessions[&ids[0]].read(cx).state, ReaderState::Epub(_)));
}

#[test]
fn production_shelf_tabs_theme_and_sheets() {
    let mut h = Harness::new("shelf");
    h.frame(|window, _| {
        assert_eq!(window.find("settings").label(), Some("设置…"));
        assert_eq!(window.find("import-files").label(), Some("导入文件…"));
        assert!(window.find("theme").bounds().size.width > px(0.));
    });
    h.click("shelf-layout");
    assert_eq!(
        h.cx.read(|cx| h.app.read(cx).settings.shelf),
        ShelfLayout::List
    );
    h.click("theme");
    assert_eq!(
        h.cx.read(|cx| h.app.read(cx).settings.appearance),
        Appearance::Dark
    );
    h.click("settings");
    h.frame(|window, cx| {
        assert!(window.find("settings-theme").visible());
        window.press("escape", cx);
    });
    h.wait(|app, _| app.overlay.is_none());
    h.frame(|window, _| assert_eq!(window.find("settings").focused(), Some(true)));
    let id = h.import("sample.epub");
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| matches!(&app.sessions[&id].read(cx).state, ReaderState::Epub(_)));
    assert_eq!(h.cx.read(|cx| h.app.read(cx).workspace.tabs.len()), 1);
    h.click("toc");
    h.frame(|window, cx| {
        assert!(window.find(("toc-epub", 0usize)).visible());
        window.click(("toc-epub", 0usize), cx);
    });
    h.wait(|app, _| app.overlay.is_none());
    h.click("settings");
    h.click("font-size-plus");
    assert_eq!(h.cx.read(|cx| h.app.read(cx).settings.epub.size), 22.);
    h.frame(|window, cx| window.press("escape", cx));
    h.click(&format!("close-{}", id.0));
    assert!(h.cx.read(|cx| h.app.read(cx).workspace.tabs.is_empty()));
    h.frame(|window, _| assert!(window.find_by_label("书架").visible()));
}

#[test]
fn production_epub_selection_copy_and_anchor_restore() {
    let mut h = Harness::new("epub");
    let id = h.import("sample.epub");
    h.frame(|window, _| {
        let cover = window
            .find(gpui_kit::SharedString::from(format!("cover-{}", id.0)))
            .bounds();
        let title = window
            .find(gpui_kit::SharedString::from(format!("open-{}", id.0)))
            .bounds();
        assert!(cover.bottom() <= title.top(), "封面不应覆盖书名");
    });
    h.click(&format!("cover-{}", id.0));
    h.wait(|app, cx| matches!(&app.sessions[&id].read(cx).state, ReaderState::Epub(_)));
    h.frame(|window, cx| {
        let bounds = window.find("previous").bounds();
        assert!(bounds.size.width > px(0.));
        let text = window.find("epub-block-1").bounds();
        let start = text.origin + point(px(24.), px(12.));
        let end = start + point(px(260.), px(0.));
        window.drag(start, end, cx);
        window.press("secondary-c", cx);
        let clipboard = cx
            .read_from_clipboard()
            .and_then(|c| c.text())
            .unwrap_or_default();
        assert!(!clipboard.trim().is_empty(), "实际拖选后剪贴板应有文字");
    });
    h.frame(|window, cx| {
        window.scroll(
            "epub-scroll",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-500.))),
            cx,
        )
    });
    h.frame(|_, _| {});
    h.cx.read(|cx| {
        if let ReaderState::Epub(pane) = &h.app.read(cx).sessions[&id].read(cx).state {
            assert!(
                pane.read(cx).scroll.top_item() > 1,
                "滚轮应改变实际滚动位置"
            );
        }
    });
    h.click("toc");
    h.frame(|window, cx| window.click(("toc-epub", 0usize), cx));
    h.wait(|app,cx|match &app.sessions[&id].read(cx).state{ReaderState::Epub(pane)=>matches!(pane.read(cx).location(),reader_core::Location::Epub{anchor,..} if anchor=="p30"),_=>false});
    h.click("settings");
    h.click("font-size-plus");
    h.click("line-height-plus");
    h.click("paragraph-gap-plus");
    h.click("content-width-minus");
    let original_font = h.cx.read(|cx| h.app.read(cx).settings.epub.font.clone());
    h.click("epub-font");
    h.frame(|window, cx| {
        window.press("down", cx);
        window.press("enter", cx);
    });
    assert_ne!(
        h.cx.read(|cx| h.app.read(cx).settings.epub.font.clone()),
        original_font
    );
    h.frame(|window, cx| window.press("escape", cx));
    h.wait(|app,cx|match &app.sessions[&id].read(cx).state{ReaderState::Epub(pane)=>matches!(pane.read(cx).location(),reader_core::Location::Epub{anchor,..} if anchor=="p30"),_=>false});
    h.click(&format!("close-{}", id.0));
    h.click(&format!("open-{}", id.0));
    h.wait(|app,cx|match &app.sessions[&id].read(cx).state{ReaderState::Epub(pane)=>matches!(pane.read(cx).location(),reader_core::Location::Epub{anchor,..} if anchor=="p30"),_=>false});
}

#[test]
fn production_pdf_navigation_zoom() {
    let mut h = Harness::new("pdf");
    let id = h.import("sample.pdf");
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| match &app.sessions[&id].read(cx).state {
        ReaderState::Pdf(pdf) => pdf.read(cx).page_count() == 3 && pdf.read(cx).has_page_text(0),
        _ => false,
    });
    let bounds =
        h.cx.read(|cx| match &h.app.read(cx).sessions[&id].read(cx).state {
            ReaderState::Pdf(pdf) => pdf.read(cx).page_bounds(0).unwrap(),
            _ => unreachable!(),
        });
    h.frame(|window, cx| {
        let start = bounds.origin
            + point(
                bounds.size.width * (42. / 600.),
                bounds.size.height * (60. / 800.),
            );
        let end = bounds.origin
            + point(
                bounds.size.width * (480. / 600.),
                bounds.size.height * (60. / 800.),
            );
        window.drag(start, end, cx);
        window.press("secondary-c", cx);
        let copied = cx
            .read_from_clipboard()
            .and_then(|c| c.text())
            .unwrap_or_default();
        assert!(
            copied.contains("Reader page 1"),
            "PDF 文字拖选复制失败：{copied:?}"
        );
    });
    h.click("next");
    h.frame(|_, _| {});
    let page =
        h.cx.read(|cx| match &h.app.read(cx).sessions[&id].read(cx).state {
            ReaderState::Pdf(pdf) => pdf.read(cx).viewport().0,
            _ => usize::MAX,
        });
    assert_eq!(page, 1);
    h.click("settings");
    h.click("pdf-zoom-plus");
    h.frame(|window, cx| window.press("escape", cx));
    h.frame(|_, _| {});
    h.cx.read(|cx| match &h.app.read(cx).sessions[&id].read(cx).state {
        ReaderState::Pdf(pdf) => {
            assert_eq!(pdf.read(cx).viewport().0, 1);
            assert_eq!(pdf.read(cx).zoom(), 1.25);
        }
        _ => panic!("PDF 会话未就绪"),
    });
    h.click("toolbar-fit-page");
    h.frame(|_, _| {});
    assert_eq!(
        h.cx.read(|cx| h.app.read(cx).settings.pdf),
        reader_core::PdfSettings::FitPage
    );
    h.click("toolbar-fit-width");
    h.frame(|_, _| {});
    assert_eq!(
        h.cx.read(|cx| h.app.read(cx).settings.pdf),
        reader_core::PdfSettings::FitWidth
    );
    h.click("toc");
    h.frame(|window, cx| window.click(("toc-pdf", 0usize), cx));
    h.wait(|app, cx| match &app.sessions[&id].read(cx).state {
        ReaderState::Pdf(pdf) => pdf.read(cx).viewport().0 == 0,
        _ => false,
    });
    h.click("next");
    h.frame(|_, _| {});
    h.click(&format!("close-{}", id.0));
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| match &app.sessions[&id].read(cx).state {
        ReaderState::Pdf(pdf) => pdf.read(cx).page_count() == 3 && pdf.read(cx).viewport().0 == 1,
        _ => false,
    });
}

#[test]
fn production_rename_delete_and_keyboard_tabs() {
    let mut h = Harness::new("management");
    let id = h.import("sample.epub");
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| matches!(&app.sessions[&id].read(cx).state, ReaderState::Epub(_)));
    h.click_label("书架");
    h.click(&format!("open-{}", id.0));
    assert_eq!(h.cx.read(|cx| h.app.read(cx).workspace.tabs.len()), 1);
    h.click_label("书架");
    h.click(&format!("rename-{}", id.0));
    h.click("rename-title");
    h.frame(|window, cx| {
        window.press("secondary-a", cx);
        window.input("新的书名", cx);
        window.click("ok", cx);
    });
    h.wait(|app, _| app.books[0].display_title == "新的书名");
    assert_eq!(
        h.cx.read(|cx| h.app.read(cx).books[0].metadata.title.clone()),
        "测试书籍"
    );
    let managed = h.cx.read(|cx| {
        let app = h.app.read(cx);
        app.paths.books().join(&app.books[0].filename)
    });
    assert!(managed.is_file());
    assert!(
        managed
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("新的书名")
    );
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| matches!(&app.sessions[&id].read(cx).state, ReaderState::Epub(_)));
    h.cx.read(|cx| match &h.app.read(cx).sessions[&id].read(cx).state {
        ReaderState::Epub(pane) => assert!(
            pane.read(cx).document.chapters[0]
                .blocks
                .iter()
                .any(|b| b.anchor == "p30")
        ),
        _ => panic!("重命名后应从托管文件重新打开 EPUB"),
    });
    h.frame(|window, cx| window.press("secondary-w", cx));
    assert!(h.cx.read(|cx| h.app.read(cx).workspace.tabs.is_empty()));
    h.click(&format!("delete-{}", id.0));
    h.click("cancel");
    assert_eq!(h.cx.read(|cx| h.app.read(cx).books.len()), 1);
    h.click(&format!("delete-{}", id.0));
    h.click("ok");
    h.wait(|app, _| app.books.is_empty());
    assert!(
        PathBuf::from(std::env::var("READER_UI_ROOT").unwrap())
            .join("sample.epub")
            .exists()
    );
}

#[test]
fn production_real_xhtml_toc_empty_chapter_and_endpoint() {
    let mut h = Harness::new("real-xhtml");
    let id = h.import("realistic.epub");
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| matches!(&app.sessions[&id].read(cx).state, ReaderState::Epub(_)));
    h.frame(|window, _| assert_eq!(window.find("previous").disabled(), Some(true)));
    h.click("next");
    h.frame(|_, _| {});
    h.cx.read(|cx| match &h.app.read(cx).sessions[&id].read(cx).state {
        ReaderState::Epub(pane) => {
            assert_eq!(pane.read(cx).chapter, 1);
            assert_eq!(
                pane.read(cx).location(),
                reader_core::Location::Epub {
                    href: "/OPS/empty.xhtml".into(),
                    anchor: String::new()
                }
            );
        }
        _ => panic!("EPUB 未就绪"),
    });
    h.click("next");
    h.frame(|_, _| {});
    h.frame(|window, cx| window.click_at("epub-block-2", point(px(38.), px(12.)), cx));
    h.wait(|app,cx| match &app.sessions[&id].read(cx).state { ReaderState::Epub(pane) => matches!(pane.read(cx).location(), reader_core::Location::Epub { anchor, .. } if anchor == "节 %20#"), _ => false });
    h.press("left");
    assert_eq!(h.reading_index(id), 2);
    h.frame(|window, cx| window.click_at("epub-block-2", point(px(38.), px(12.)), cx));
    h.wait(|app,cx| match &app.sessions[&id].read(cx).state { ReaderState::Epub(pane) => matches!(pane.read(cx).location(), reader_core::Location::Epub { anchor, .. } if anchor == "节 %20#"), _ => false });
    h.click("toc");
    h.frame(|window, _| {
        assert!(
            window.find(("toc-epub", 1usize)).bounds().left()
                > window.find(("toc-epub", 0usize)).bounds().left()
        );
        assert!(window.try_find(("toc-epub", 2usize)).is_none());
        assert_eq!(
            window.find(("bookmark-toc-epub", 1usize)).selected(),
            Some(true)
        );
    });
    assert!(h.cx.read(|cx| h.app.read(cx).overlay.is_some()));
    h.frame(|window, cx| window.click(("toc-epub", 1usize), cx));
    h.wait(|app,cx| match &app.sessions[&id].read(cx).state { ReaderState::Epub(pane) => matches!(pane.read(cx).location(), reader_core::Location::Epub { anchor, .. } if anchor == "节 %20#"), _ => false });
    let before =
        h.cx.read(|cx| h.app.read(cx).sessions[&id].read(cx).position.clone());
    h.frame(|window, cx| {
        assert_eq!(window.find("next").disabled(), Some(true));
        window.click("next", cx);
    });
    assert_eq!(
        before,
        h.cx.read(|cx| h.app.read(cx).sessions[&id].read(cx).position.clone())
    );
    h.click(&format!("close-{}", id.0));
    h.click(&format!("open-{}", id.0));
    h.wait(|app,cx| match &app.sessions[&id].read(cx).state { ReaderState::Epub(pane) => matches!(pane.read(cx).location(), reader_core::Location::Epub { anchor, .. } if anchor == "节 %20#"), _ => false });
    h.frame(|window, cx| window.press("secondary-q", cx));
    h.wait(|app, _| app.shutdown == crate::app::Shutdown::Saved);
    let path = h.cx.read(|cx| {
        h.app
            .read(cx)
            .paths
            .books()
            .join(&h.app.read(cx).books[0].filename)
    });
    assert!(path.is_file());
    drop(h);
    let mut restarted = Harness::new("real-xhtml");
    restarted.wait(|app,cx| app.workspace.active == Some(id) && match &app.sessions[&id].read(cx).state { ReaderState::Epub(pane) => matches!(pane.read(cx).location(), reader_core::Location::Epub { anchor, .. } if anchor == "节 %20#"), _ => false });
}

#[test]
fn production_epub_bookmarks_follow_reading_and_keyboard_navigation() {
    let mut h = Harness::new("epub-bookmarks");
    let id = h.import("bookmarks.epub");
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| matches!(&app.sessions[&id].read(cx).state, ReaderState::Epub(_)));
    h.frame(|window, cx| window.click_at("epub-block-0", point(px(40.), px(10.)), cx));
    for (key, chapter) in [
        ("right", 1),
        ("left", 0),
        ("pagedown", 1),
        ("pagedown", 1),
        ("pageup", 0),
        ("pageup", 0),
    ] {
        h.press(key);
        assert_eq!(h.reading_index(id), chapter, "{key}");
    }
    h.press("shift-right");
    assert_eq!(h.reading_index(id), 0);
    h.frame(|window, cx| {
        window.scroll(
            "epub-scroll",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-3000.))),
            cx,
        )
    });
    h.frame(|_, _| {});
    let paragraph =
        h.cx.read(|cx| match &h.app.read(cx).sessions[&id].read(cx).state {
            ReaderState::Epub(pane) => match pane.read(cx).location() {
                reader_core::Location::Epub { anchor, .. } => {
                    anchor.strip_prefix('p').unwrap().parse::<usize>().unwrap()
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        });
    assert!(paragraph > 20);
    for _ in 0..2 {
        h.click("toc");
        h.frame(|window, cx| {
            assert_eq!(window.find("toc").label(), Some("书签…"));
            assert!(window.try_find(("toc-epub", 0usize)).is_none());
            let current = window.find(("bookmark-toc-epub", paragraph + 1));
            assert_eq!(current.selected(), Some(true));
            let viewport = window.find("bookmarks-scroll").bounds();
            assert!(
                current.visible(),
                "current={:?} viewport={viewport:?}",
                current.bounds()
            );
            assert!(current.bounds().top() >= viewport.top());
            assert!(current.bounds().bottom() <= viewport.bottom());
            window.scroll(
                "bookmarks-scroll",
                gpui_kit::ScrollDelta::Pixels(point(px(0.), px(5000.))),
                cx,
            );
        });
        h.press("pagedown");
        assert_eq!(h.reading_index(id), 0);
        h.press("escape");
        h.wait(|app, _| app.overlay.is_none());
    }
    h.click("next");
    assert_eq!(h.reading_index(id), 1);
    h.click("toc");
    h.frame(|window, _| {
        let current = window.find(("bookmark-toc-epub", 61usize));
        assert_eq!(current.selected(), Some(true));
        assert!(current.visible());
    });
    h.press("left");
    assert_eq!(h.reading_index(id), 1);
    h.press("escape");
    h.click("settings");
    for key in ["left", "right", "pageup", "pagedown"] {
        h.press(key);
        assert_eq!(h.reading_index(id), 1);
    }
    h.press("escape");
    let app = h.app.clone();
    h.frame(|window, cx| app.update(cx, |app, cx| app.rename_dialog(id, window, cx)));
    h.click("rename-title");
    h.frame(|window, cx| {
        window.press("secondary-a", cx);
        window.input("abc", cx);
        window.press("left", cx);
        window.press("backspace", cx);
        assert_eq!(window.find("rename-title").value(), Some("ac"));
    });
    for key in ["left", "right", "pageup", "pagedown"] {
        h.press(key);
        assert_eq!(h.reading_index(id), 1);
    }
    h.click("cancel");
    h.press("left");
    assert_eq!(h.reading_index(id), 0);
}

#[test]
fn production_bookmarks_hide_unresolved_entries_and_show_empty_state() {
    for (name, filename) in [
        ("empty-epub-bookmarks", "empty-bookmarks.epub"),
        ("empty-pdf-bookmarks", "empty-bookmarks.pdf"),
    ] {
        let mut h = Harness::new(name);
        let id = h.import(filename);
        h.click(&format!("open-{}", id.0));
        h.wait(|app, cx| match &app.sessions[&id].read(cx).state {
            ReaderState::Epub(_) => true,
            ReaderState::Pdf(pdf) => pdf.read(cx).page_count() == 3,
            _ => false,
        });
        h.click("toc");
        h.frame(|window, _| {
            assert!(window.try_find(("toc-epub", 0usize)).is_none());
            assert!(window.try_find(("toc-pdf", 0usize)).is_none());
            assert!(window.find("bookmarks-empty").visible());
            assert_eq!(
                window.find("bookmarks-empty").label(),
                Some("这本书没有可跳转的书签")
            );
        });
        h.press("escape");
        h.wait(|app, _| app.overlay.is_none());
        h.frame(|window, _| assert_eq!(window.find("toc").focused(), Some(true)));
    }
}

#[test]
fn production_pdf_bookmarks_and_focused_viewer_share_navigation() {
    let mut h = Harness::new("pdf-bookmarks");
    let id = h.import("bookmarks.pdf");
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| match &app.sessions[&id].read(cx).state {
        ReaderState::Pdf(pdf) => pdf.read(cx).page_count() == 3 && pdf.read(cx).is_viewport_ready(),
        _ => false,
    });
    h.frame(|window, cx| {
        use gpui_kit::test::TestEventExt;
        let center = point(px(700.), px(400.));
        window.pointer_down(center, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(center, gpui_kit::MouseButton::Left, cx);
    });
    for (key, page) in [
        ("pagedown", 1),
        ("left", 0),
        ("right", 1),
        ("pagedown", 2),
        ("right", 2),
        ("pageup", 1),
    ] {
        h.press(key);
        assert_eq!(h.reading_index(id), page, "{key}");
    }
    h.press("secondary-left");
    assert_eq!(h.reading_index(id), 1);
    for _ in 0..2 {
        h.click("toc");
        h.frame(|window, _| {
            assert!(window.try_find(("toc-pdf", 0usize)).is_none());
            let current = window.find(("bookmark-toc-pdf", 41usize));
            assert_eq!(current.selected(), Some(true));
            let viewport = window.find("bookmarks-scroll").bounds();
            assert!(
                current.visible(),
                "current={:?} viewport={viewport:?}",
                current.bounds()
            );
            assert!(current.bounds().top() >= viewport.top());
            assert!(current.bounds().bottom() <= viewport.bottom());
        });
        h.press("pageup");
        assert_eq!(h.reading_index(id), 1);
        h.press("escape");
        h.wait(|app, _| app.overlay.is_none());
    }
    h.click("next");
    assert_eq!(h.reading_index(id), 2);
    h.click("toc");
    h.frame(|window, cx| {
        assert_eq!(
            window.find(("bookmark-toc-pdf", 42usize)).selected(),
            Some(true)
        );
        window.click(("toc-pdf", 41usize), cx);
    });
    assert_eq!(h.reading_index(id), 1);
    h.press("right");
    assert_eq!(h.reading_index(id), 2);
}

#[test]
fn production_folder_import_failure_feedback_and_delete_open_book() {
    let mut h = Harness::new("folder");
    let root = PathBuf::from(std::env::var("READER_UI_ROOT").unwrap());
    let folder = root.join("folder-sources");
    std::fs::create_dir(&folder).unwrap();
    std::fs::copy(root.join("sample.epub"), folder.join("book.epub")).unwrap();
    std::fs::write(folder.join("notes.txt"), "非书籍").unwrap();
    std::fs::write(folder.join("bad.pdf"), "损坏 PDF").unwrap();
    h.click("import-folder");
    h.cx.simulate_path_prompt_response(|options| {
        assert!(options.directories && !options.files);
        Some(vec![folder])
    });
    h.wait(|app, _| !app.busy && app.books.len() == 1);
    h.cx.read(|cx| {
        let status = &h.app.read(cx).status;
        assert!(
            status.contains("1 项非书籍")
                && status.contains("失败 1 项")
                && status.contains("bad.pdf"),
            "{status}"
        );
    });
    let id = h.cx.read(|cx| h.app.read(cx).books[0].id);
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| matches!(&app.sessions[&id].read(cx).state, ReaderState::Epub(_)));
    h.click_label("书架");
    h.click(&format!("delete-{}", id.0));
    h.click("ok");
    h.wait(|app, _| app.books.is_empty());
    h.cx.read(|cx| {
        assert!(h.app.read(cx).sessions.is_empty());
        assert!(h.app.read(cx).workspace.tabs.is_empty());
        assert_eq!(
            std::fs::read_dir(h.app.read(cx).paths.books())
                .unwrap()
                .count(),
            0
        );
    });
}

#[test]
fn production_failed_rename_keeps_latest_position_and_original() {
    let mut h = Harness::new("rename-failure");
    let id = h.import("sample.epub");
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| matches!(&app.sessions[&id].read(cx).state, ReaderState::Epub(_)));
    h.click("toc");
    h.frame(|window, cx| window.click(("toc-epub", 0usize), cx));
    h.wait(|app,cx| matches!(&app.sessions[&id].read(cx).position, Some(reader_core::Location::Epub { anchor, .. }) if anchor == "p30"));
    let (original, collision) = h.cx.read(|cx| {
        let app = h.app.read(cx);
        let book = &app.books[0];
        (
            app.paths.books().join(&book.filename),
            app.paths
                .books()
                .join(format!("{}-失败书名.epub", book.digest)),
        )
    });
    std::fs::create_dir(&collision).unwrap();
    h.click_label("书架");
    h.click(&format!("rename-{}", id.0));
    h.click("rename-title");
    h.frame(|window, cx| {
        window.press("secondary-a", cx);
        window.input("失败书名", cx);
        window.click("ok", cx);
    });
    h.wait(|app, _| app.status.contains("重命名失败"));
    assert!(original.is_file());
    h.click(&format!("open-{}", id.0));
    h.wait(|app,cx| match &app.sessions[&id].read(cx).state { ReaderState::Epub(pane) => matches!(pane.read(cx).location(), reader_core::Location::Epub { anchor, .. } if anchor == "p30"), _ => false });
    h.click(&format!("close-{}", id.0));
    h.click(&format!("open-{}", id.0));
    h.wait(|app,cx| match &app.sessions[&id].read(cx).state { ReaderState::Epub(pane) => matches!(pane.read(cx).location(), reader_core::Location::Epub { anchor, .. } if anchor == "p30"), _ => false });
}

#[test]
fn production_quit_waits_for_slow_queue_and_saves_latest_snapshot() {
    let mut h = Harness::new("slow-quit");
    let id = h.import("sample.epub");
    h.click(&format!("open-{}", id.0));
    h.wait(|app, cx| matches!(&app.sessions[&id].read(cx).state, ReaderState::Epub(_)));
    h.click("toc");
    h.frame(|window, cx| window.click(("toc-epub", 0usize), cx));
    h.wait(|app,cx| matches!(&app.sessions[&id].read(cx).position, Some(reader_core::Location::Epub { anchor, .. }) if anchor == "p30"));
    h.click("theme");
    let service = h.cx.read(|cx| h.app.read(cx).service.clone());
    let (release, blocked) = std::sync::mpsc::channel();
    let (started, waiting) = std::sync::mpsc::channel();
    let pending = service.submit(move |_| {
        started.send(()).unwrap();
        blocked.recv().unwrap();
        Ok(())
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    h.frame(|window, cx| window.press("secondary-q", cx));
    let deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < deadline {
        h.frame(|_, _| {});
    }
    assert!(h.cx.read(|cx| h.app.read(cx).shutdown == crate::app::Shutdown::Saving));
    h.frame(|window, cx| window.press("secondary-q", cx));
    release.send(()).unwrap();
    futures::executor::block_on(pending).unwrap();
    h.wait(|app, _| app.shutdown == crate::app::Shutdown::Saved);
    let (books, settings, workspace) = futures::executor::block_on(
        service.submit(|library| Ok((library.books()?, library.settings()?, library.workspace()?))),
    )
    .unwrap();
    assert!(
        matches!(&books[0].location, Some(reader_core::Location::Epub { anchor, .. }) if anchor == "p30")
    );
    assert_eq!(settings.appearance, Appearance::Dark);
    assert_eq!(workspace.active, Some(id));
    assert_eq!(workspace.tabs, vec![id]);
}

#[test]
fn production_quit_save_failure_stays_open_and_can_retry() {
    let mut h = Harness::new("quit-failure");
    let path =
        h.cx.read(|cx| h.app.read(cx).paths.config.join("settings.json"));
    std::fs::create_dir(&path).unwrap();
    h.frame(|window, cx| window.press("secondary-q", cx));
    h.wait(|app, _| {
        app.shutdown == crate::app::Shutdown::Running && app.status.contains("退出保存失败")
    });
    std::fs::remove_dir(&path).unwrap();
    h.frame(|window, cx| window.press("secondary-q", cx));
    h.wait(|app, _| app.shutdown == crate::app::Shutdown::Saved);
    assert!(path.is_file());
}

#[test]
fn production_corrupt_state_uses_defaults_with_visible_diagnostic() {
    let root = PathBuf::from(std::env::var("READER_UI_ROOT").unwrap()).join("corrupt-state");
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::create_dir_all(root.join("state")).unwrap();
    std::fs::write(root.join("config/settings.json"), b"broken settings").unwrap();
    std::fs::write(root.join("state/workspace.json"), b"broken workspace").unwrap();
    let mut h = Harness::new("corrupt-state");
    h.cx.read(|cx| {
        let app = h.app.read(cx);
        assert!(app.status.contains("已损坏，使用默认值"));
        assert!(app.status.contains("settings.json.corrupt-0"));
        assert!(app.status.contains("workspace.json.corrupt-0"));
        assert_eq!(app.settings.epub.font, ".SystemUIFont");
        assert!(app.workspace.tabs.is_empty());
    });
    h.click("settings");
    h.frame(|window, _| assert!(window.find("settings-theme").visible()));
    assert_eq!(
        std::fs::read(root.join("config/settings.json.corrupt-0")).unwrap(),
        b"broken settings"
    );
    assert_eq!(
        std::fs::read(root.join("state/workspace.json.corrupt-0")).unwrap(),
        b"broken workspace"
    );
}
