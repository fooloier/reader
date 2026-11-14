# gpui-base 本地补丁

基于 crates.io `gpui-base 0.9.1`，上游为 [longbridge/gpui-kit](https://github.com/longbridge/gpui-kit)。通过工作区 `[patch.crates-io]` 接入；保留包内源码、许可证与构建清单，不修改 Cargo registry。清单移除未随包接入的外部测试与 benchmark 目标。

- `text/inline.rs` 从真实 glyph box 记录阅读顺序最后一个选中非空白字形，正确处理软换行与尾部空白。
- `text/state.rs` 和 `text/selection_adapter.rs` 暴露同一已绘制选区的文字、来源范围、内容版本与末字形快照。选区拖动中或内容布局尚未有效时不返回快照。
- `text_selection.rs` 允许控件登记保留选区的 hitbox，在 capture 阶段准备新选择前识别。登记跟随 retained element 生命周期，普通外部点击仍按原逻辑清空。

升级时核对上述接口是否已被上游替代，先迁移调用再删除补丁。验证入口为 Reader 生产 UI 测试中的 `production_epub_translation_*`，应保留软换行、反向、跨块、按钮点击、键盘和重新排版路径。
