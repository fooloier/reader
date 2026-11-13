# PDF 阅读

验证用户可见的PDF 阅读行为。

## Sub-features

- pdf.interaction：入口动作与反馈。
- pdf.result：最终状态及适用的恢复行为。

## How to get to it (user POV)

书架打开 PDF；阅读工具栏翻页、适宽、整页、目录；设置缩放。

## Driving it with TestWindowExt

Preconditions: 从 Launch 的空隔离目录和生成夹具开始。

- 执行：`python3 .agents/skills/verify-reader/scripts/verify.py ui`，检查日志中的 `production_pdf_navigation_zoom、production_pdf_bookmarks_and_focused_viewer_share_navigation` 全部通过。
- 动作：打开 sample.pdf，点击 next、toolbar-fit-page、toolbar-fit-width；settings 内点击 pdf-zoom-plus；拖选并复制；关闭重开。
- 结果：页码变化、缩放生效、复制内容正确，重开恢复页码；书签跳转与页码同步。
- 涉及磁盘结果时执行 `python3 .agents/skills/verify-reader/scripts/verify.py integration`；原生窗口证据使用 `native`。

## Gotchas

PDF 无密码输入流程；无效书签应隐藏或禁用，不能当作可点击目标。 缺失前提时记录失败，不缩减断言。上面列出的入口只有在对应用例实际操作后才能标记通过。
