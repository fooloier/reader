# PDF 阅读

验证用户可见的PDF 阅读行为。

## Sub-features

- pdf.interaction：入口动作与反馈。
- pdf.result：最终状态及适用的恢复行为。

## How to get to it (user POV)

书架打开 PDF；阅读工具栏翻页、适宽、整页、书签；设置缩放。

## Driving it with TestWindowExt

Preconditions: 从 Launch 的空隔离目录和生成夹具开始。

- 执行：`python3 .agents/skills/verify-reader/scripts/verify.py ui`，检查日志中的 `production_pdf_navigation_zoom、production_pdf_bookmarks_and_focused_viewer_share_navigation` 全部通过。
- 动作：打开 sample.pdf，点击 next、toolbar-fit-page、toolbar-fit-width；settings 内点击 pdf-zoom-plus；拖选并复制；关闭重开。
- 结果：页码变化、缩放生效、复制内容正确，重开恢复页码；书签跳转与页码同步。
- 空书签：运行 `production_bookmarks_hide_unresolved_entries_and_show_empty_state`，确认无效条目隐藏、无有效条目时显示“这本书没有可跳转的书签”，Escape 关闭后焦点返回书签按钮。
- 涉及磁盘结果时执行 `python3 .agents/skills/verify-reader/scripts/verify.py integration`；离屏截图使用 `native`。

## 选中翻译

同一 `ui` 命令运行生产事件测试。`production_pdf_translation_release_reverse_and_close` 覆盖释放点、反向与同文重选、旧结果不重开、点击后翻译快照、底边翻转与视口钳制、真实滚动条拖动、分页键、Escape、翻页隐藏和关闭会话。`production_pdf_pending_translation_capture_and_cancellation` 逐步推进真实 PDF 后台提取，用真实拖选验证延迟文字捕获，以及新按下、滚动、关闭后的取消。

测试用本地 Rust HTTP 服务注入生产 TranslationService，不发送公共网络请求。公共接口检查另运行 `cargo test --locked -p reader-runtime translation::tests::public_api_smoke -- --ignored --nocapture`，报告实际接口结果。`production_translation_retry_long_result_and_inflight_tab_close` 另验证限流后的重试、长译文滚动与复制、滚动清除、空选区清除和清除缓存反馈。原生像素仍单独使用 `native`，不能从事件断言推断像素通过。

## Gotchas

PDF 无密码输入流程；无效书签应隐藏或禁用，不能当作可点击目标。 缺失前提时记录失败，不缩减断言。上面列出的入口只有在对应用例实际操作后才能标记通过。
