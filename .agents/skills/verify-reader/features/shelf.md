# 书架与显示

验证用户可见的书架与显示行为。

## Sub-features

- shelf.interaction：入口动作与反馈。
- shelf.result：最终状态及适用的恢复行为。

## How to get to it (user POV)

工具栏布局、主题按钮；设置按钮或 Ctrl+,；书卡和列表书名。

## Driving it with TestWindowExt

Preconditions: 从 Launch 的空隔离目录和生成夹具开始。

- 执行：`python3 .agents/skills/verify-reader/scripts/verify.py ui`，检查日志中的 `production_shelf_tabs_theme_and_sheets、production_shelf_cards_reflow_with_consistent_columns` 全部通过。
- 动作：点击 shelf-layout、theme、settings；Escape 关闭设置；点击 open-{id}。
- 结果：布局切换为列表，主题变深；设置关闭后焦点回到 settings；同书不重复打开标签。
- 涉及磁盘结果时执行 `python3 .agents/skills/verify-reader/scripts/verify.py integration`；原生窗口证据使用 `native`。

## Gotchas

设置快捷键入口需单独确认；原生预置主题不证明切换动作。 缺失前提时记录失败，不缩减断言。上面列出的入口只有在对应用例实际操作后才能标记通过。
