# 退出与恢复

验证用户可见的退出与恢复行为。

## Sub-features

- persistence.interaction：入口动作与反馈。
- persistence.result：最终状态及适用的恢复行为。

## How to get to it (user POV)

Ctrl+Q 或关闭窗口；关闭书籍标签 Ctrl+W 后重开。

## Driving it with TestWindowExt

Preconditions: 从 Launch 的空隔离目录和生成夹具开始。

- 执行：`python3 .agents/skills/verify-reader/scripts/verify.py ui`，检查日志中的 `production_quit_waits_for_slow_queue_and_saves_latest_snapshot、production_quit_save_failure_stays_open_and_can_retry、production_corrupt_state_uses_defaults_with_visible_diagnostic` 全部通过。
- 动作：运行退出用例，在排队任务未完成时按退出；保存失败后修复夹具权限/路径并重试；native 检查真实窗口关闭和恢复。
- 结果：慢任务完成前保持打开，最终快照落盘；保存失败显示错误并允许重试；原生退出码为零且读取进度符合预期。
- 涉及磁盘结果时执行 `python3 .agents/skills/verify-reader/scripts/verify.py integration`；原生窗口证据使用 `native`。

## Gotchas

故障注入仅限隔离测试目录；真实窗口慢队列场景未验证，不能从 UI 测试外推。 缺失前提时记录失败，不缩减断言。上面列出的入口只有在对应用例实际操作后才能标记通过。
