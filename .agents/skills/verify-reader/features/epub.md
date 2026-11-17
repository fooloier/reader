# EPUB 阅读

验证用户可见的EPUB 阅读行为。

## Sub-features

- epub.interaction：入口动作与反馈。
- epub.result：最终状态及适用的恢复行为。

## How to get to it (user POV)

书卡封面、列表书名；阅读工具栏书签、上一章、下一章、设置；正文链接。

## Driving it with TestWindowExt

Preconditions: 从 Launch 的空隔离目录和生成夹具开始。

- 执行：`python3 .agents/skills/verify-reader/scripts/verify.py ui`，检查日志中的 `production_epub_selection_copy_and_anchor_restore、production_real_xhtml_toc_empty_chapter_and_endpoint、production_epub_bookmarks_follow_reading_and_keyboard_navigation` 全部通过。
- 动作：打开 sample.epub 或 realistic.epub；拖选正文并复制；点击 toc、next、font-size-plus；关闭 close-{id} 后重新打开。
- 结果：拖选复制得到非空正文；章节与书签同步；排版变化；重新打开恢复锚点；空章节正常显示。
- 空书签：运行 `production_bookmarks_hide_unresolved_entries_and_show_empty_state`，确认无效条目隐藏、无有效条目时显示“这本书没有可跳转的书签”，Escape 关闭后焦点返回书签按钮。
- 涉及磁盘结果时执行 `python3 .agents/skills/verify-reader/scripts/verify.py integration`；原生窗口证据使用 `native`。

## 选中翻译

同一 `ui` 命令运行生产事件测试。`production_epub_translation_release_snapshot_keyboard_and_reflow` 与 `production_epub_translation_cross_block_stale_and_dismissal` 覆盖软换行、正反向释放点、鼠标移动后按钮固定、跨块文本与 Ctrl+C 一致、点击清除原生高亮后仍翻译快照、复制、Tab/Enter、无快照的翻译 Action、旧请求结果、弹窗外首次拖选、下一次按下取消待提交捕获、拖选边缘自动滚动、滚动和重排。

测试用本地 Rust HTTP 服务注入生产 TranslationService，不发送公共网络请求。公共接口检查另运行 `cargo test --locked -p reader-runtime translation::tests::public_api_smoke -- --ignored --nocapture`，报告实际接口结果。原生像素仍单独使用 `native`，不能从事件断言推断像素通过。

## Gotchas

夹具由 UI 编排生成；复杂出版物和全部字体未覆盖；native 的锚点由预置产生，只证明恢复。 缺失前提时记录失败，不缩减断言。上面列出的入口只有在对应用例实际操作后才能标记通过。
