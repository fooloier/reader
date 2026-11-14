# 本地修改

来源为 crates.io gpui-pdf 0.7.0，MIT 许可证见 LICENSE。保留原始源码与归属。

- 使用 gpui-pre 0.3.12 的 Modifiers::secondary(cx) 签名。
- 增加宿主工具栏模式，隐藏内置工具栏和悬浮回顶按钮，预取附近页面文字并保留复制选择模式。禁用宿主模式中不可见的高亮和页码输入快捷键。
- 增加 Ready、ViewportChanged 事件与页面数、目录、缩放、视口读取及延迟恢复接口。
- 视口恢复等待文档与布局就绪，缩放保留页内比例。
- 附近页面之外的提取文字缓存会淘汰；异步结果不重新插入已淘汰页面。
- 提供页面实际边界和文字就绪读取，用于事件驱动的复制测试。
- 未激活标签的待恢复视口可读，避免退出时覆盖进度。
- 翻译加载和错误提示。未开启 search 或 forms 功能。

补丁只存在于 vendor，不修改 Cargo 注册表。

## 选中翻译宿主接口

`PageText::selected_glyph` 与选中文本共用 nearest-run 区间，返回阅读顺序最后的非空白 run。`PdfView::selection_snapshot` 将该字形映射到窗口坐标；`PdfEvent::SelectionChanged` 在开始清空、完成选择和延迟提取结束时通知宿主。仍使用原有单页绘制顺序近似，不扩展复杂 PDF 阅读排序。

升级时保留 `text.rs` 的反向、多行与尾空白测试，并运行 Reader 的 `production_pdf_translation_final_glyph_reverse_and_close`。

选中文本拼接在显式空白 run 前后保留最后真实字形的基线，避免跨行空白吞掉换行。
