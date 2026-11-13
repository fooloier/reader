# 源码导航

按问题选择入口，沿调用与测试读取当前行为。这里不维护实现摘要。

| 要解决的问题 | 先读实现 | 再读验证 |
| --- | --- | --- |
| 启动、数据目录 | [入口](../src/main.rs)、[路径](../crates/runtime/src/paths.rs) | [领域与路径测试](../tests/unit/test_domain.py) |
| 书籍身份、位置、设置 | [领域类型](../crates/core/src/lib.rs) | 同文件中的 Rust 测试 |
| 导入、改名、删除、恢复 | [Library](../crates/runtime/src/library.rs)、[格式解析](../crates/runtime/src/formats.rs) | [书库集成测试](../tests/integration/test_library.py) |
| 后台任务、标签、退出保存 | [Service](../crates/runtime/src/service.rs)、[应用](../src/app.rs) | [生产 UI 测试](../src/ui_tests.rs) |
| 书架、菜单、工具栏 | [书架](../src/shelf.rs)、[工具栏](../src/toolbar.rs)、[弹层](../src/overlays.rs) | [生产 UI 测试](../src/ui_tests.rs) |
| 阅读、目录、排版、图片 | [阅读会话](../src/reading/mod.rs)、[EPUB 解析](../crates/runtime/src/epub.rs)、[设置](../src/settings.rs) | [生产 UI 测试](../src/ui_tests.rs) |
| PDF 宿主接入或升级 | [阅读会话](../src/reading/mod.rs)、[本地补丁来源](../vendor/gpui-pdf/PATCHES.md) | [生产 UI 测试](../src/ui_tests.rs) |
| 构建依赖与特性 | [工作区清单](../Cargo.toml)、[锁文件](../Cargo.lock) | [测试入口](../tests/run.py) |

需要运行证明时读 [验证技能](../.agents/skills/verify-reader/SKILL.md)；需要解释历史选择时读 [决策背景](decisions.md)。
