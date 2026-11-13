# 导入与书库管理

验证用户可见的导入与书库管理行为。

## Sub-features

- library.interaction：入口动作与反馈。
- library.result：最终状态及适用的恢复行为。

## How to get to it (user POV)

文件/文件夹图标或 Ctrl+O / Ctrl+Shift+O；书卡菜单和列表右键菜单。

## Driving it with TestWindowExt

Preconditions: 从 Launch 的空隔离目录和生成夹具开始。

- 执行：`python3 .agents/skills/verify-reader/scripts/verify.py ui`，检查日志中的 `production_folder_import_failure_feedback_and_delete_open_book、production_rename_delete_and_keyboard_tabs、production_shelf_table_resize_and_row_commands` 全部通过。
- 动作：点击 import-files 或 import-folder，返回夹具路径；通过 book-menu-{id} 或行右键选择 重命名… / 删除…；rename-title 输入，ok 确认或 cancel 取消。
- 结果：导入成功和失败统计可见；取消保留书籍；确认删除移除书籍与打开标签；源文件完整。
- 涉及磁盘结果时执行 `python3 .agents/skills/verify-reader/scripts/verify.py integration`；原生窗口证据使用 `native`。

## Gotchas

系统选择器只模拟边界返回，真实选择器与导入快捷键未完整验证；源文件与托管文件检查需 integration。 缺失前提时记录失败，不缩减断言。上面列出的入口只有在对应用例实际操作后才能标记通过。
