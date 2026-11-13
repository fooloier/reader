# EPUB 阅读

验证用户可见的EPUB 阅读行为。

## Sub-features

- epub.interaction：入口动作与反馈。
- epub.result：最终状态及适用的恢复行为。

## How to get to it (user POV)

书卡封面、列表书名；阅读工具栏目录、上一章、下一章、设置；正文链接。

## Driving it with TestWindowExt

Preconditions: 从 Launch 的空隔离目录和生成夹具开始。

- 执行：`python3 .agents/skills/verify-reader/scripts/verify.py ui`，检查日志中的 `production_epub_selection_copy_and_anchor_restore、production_real_xhtml_toc_empty_chapter_and_endpoint、production_epub_bookmarks_follow_reading_and_keyboard_navigation` 全部通过。
- 动作：打开 sample.epub 或 realistic.epub；拖选正文并复制；点击 toc、next、font-size-plus；关闭 close-{id} 后重新打开。
- 结果：复制文本正确；章节与书签同步；排版变化；重新打开恢复锚点；空章节正常显示。
- 涉及磁盘结果时执行 `python3 .agents/skills/verify-reader/scripts/verify.py integration`；原生窗口证据使用 `native`。

## Gotchas

夹具由 UI 编排生成；复杂出版物和全部字体未覆盖；native 的锚点由预置产生，只证明恢复。 缺失前提时记录失败，不缩减断言。上面列出的入口只有在对应用例实际操作后才能标记通过。
