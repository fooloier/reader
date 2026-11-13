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
            let (_window, _view) = gpui_kit::open_window(
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
            #[cfg(feature = "test-support")]
            if let Some(path) = std::env::var_os("READER_READY_FILE") {
                cx.spawn(async move |cx| {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
                    while std::time::Instant::now() < deadline {
                        let ready = _view.read_with(cx, |view, cx| view.is_ready(cx));
                        if ready {
                            let _ = cx.update_window(_window, |_, window, _| {
                                window.on_next_frame(move |_, _| {
                                    let _ = std::fs::write(path, b"ready");
                                });
                            });
                            return;
                        }
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(20))
                            .await;
                    }
                })
                .detach();
            }
            cx.activate(true);
        });
    Ok(())
}
