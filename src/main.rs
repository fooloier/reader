use gpui_kit::*;
use reader::app::{Boot, ReaderApp};
use reader_runtime::{AppPaths, Library};

fn main() -> anyhow::Result<()> {
    let paths = AppPaths::native()?;
    let boot = Boot::load(Library::open(paths)?)?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            reader::app::init(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1100.), px(800.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(680.), px(480.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Reader 阅读器".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                move |window, cx| cx.new(|cx| ReaderApp::new(boot, window, cx)),
            )
            .expect("无法打开阅读器窗口");
            cx.activate(true);
        });
    Ok(())
}
