use crate::app::{Boot, ReaderApp, Shutdown};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, HeadlessAppContext, WindowBounds, WindowOptions, platform, px, size,
};
use reader_runtime::{AppPaths, Library};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
#[ignore = "通过 verify.py native 预置隔离书库和截图路径"]
fn capture_reader() -> anyhow::Result<()> {
    let path = std::env::var("READER_SCREENSHOT")?;
    let boot = Boot::load(Library::open(AppPaths::native()?)?)?;
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(gpui_kit::assets::Assets),
        platform::current_headless_renderer,
    );
    let (window, app) = cx.update(|cx| {
        crate::app::init(cx);
        cx.set_reduce_motion(true);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1100.), px(800.)),
                ))),
                show: false,
                focus: false,
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| ReaderApp::new(boot, window, cx)),
        )
    })?;
    let frame = Duration::from_millis(16);
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut previous = None;
    let mut matches = 0;
    loop {
        anyhow::ensure!(Instant::now() < deadline, "Reader 未在期限内生成稳定的画面");
        cx.advance_clock(frame);
        cx.run_until_parked();
        let ready = cx.update_window(window, |_, window, cx| {
            window.simulate_next_frame(cx);
            window.render_frame(cx);
            app.read(cx).is_ready(cx)
        })?;
        if ready {
            let image = cx.capture_screenshot(window)?;
            if previous.as_ref() == Some(&image) {
                matches += 1;
                if matches == 2 {
                    image.save(path)?;
                    break;
                }
            } else {
                matches = 0;
            }
            previous = Some(image);
        }
    }
    cx.update_window(window, |_, window, cx| window.press("secondary-q", cx))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cx.update(|cx| app.read(cx).shutdown == Shutdown::Saved) {
        anyhow::ensure!(Instant::now() < deadline, "退出保存超时");
        cx.run_until_parked();
        cx.advance_clock(frame);
    }
    Ok(())
}
