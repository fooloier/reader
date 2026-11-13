---
name: verify-reader
description: 验证 Reader 原生 GPUI 桌面应用的书库操作、PDF/EPUB 阅读、设置和退出持久化；修改交互或需要真实运行证据时使用。
---

# Reader 验证

在仓库根目录执行。先读 [功能地图](features/README.md)，选择所需入口。主要表面是原生桌面；reader-driver 是集成测试接口，不是用户 CLI。

## Launch

```sh
cargo build --locked --workspace --all-targets --features test-support
python3 .agents/skills/verify-reader/scripts/verify.py doctor
python3 .agents/skills/verify-reader/scripts/verify.py ui
```

需要 Rust、系统图形开发库、平台文本系统和中文字体。UI 编排在临时目录生成 PDF、EPUB、目录和长标题夹具，逐项创建空隔离书库及生产 ReaderApp。Harness 渲染首帧后才操作，异步导入等通过最长 15 秒状态等待确认可用。无需账号、端口或手工迁移。命令成功应包含 17 项 Rust 测试通过。

需要真实窗口像素或启动恢复证明时，再运行：

```sh
python3 .agents/skills/verify-reader/scripts/verify.py native
```

要求 Wayland、Hyprland Lua dispatch、hyprctl 和 grim。脚本隔离四个 XDG 目录，生成书籍，通过 reader-driver 预置状态；窗口必须属于新 PID、已映射、READER_READY_FILE 存在，且连续三张截图相同才算就绪。普通启动命令是 `cargo run --locked --bin reader`，验证使用上面的隔离编排，不操作用户书库。

## Doctor

`python3 .agents/skills/verify-reader/scripts/verify.py doctor` 是只读检查，确认路径、二进制和离线 Cargo 元数据。它不证明二进制新鲜或桌面可用，必须先成功完成本轮构建。原生失败再读 `hyprctl clients -j`，对照本次日志；仅允许驱动脚本创建的 PID。

## Drive

`ui` 运行现有 TestWindowExt 事件驱动，稳定 ID 与功能断言见地图及 `src/ui_tests.rs`。导入点击真实 `import-files`，仅操作系统路径选择边界使用模拟返回。保持真实点击、键盘、滚轮和拖选；禁止用内部状态赋值代替待验证动作。

持久化和源文件完整性补充运行：

```sh
python3 .agents/skills/verify-reader/scripts/verify.py integration
```

UI 测试覆盖事件路径，integration 覆盖真实 Library 磁盘结果，native 覆盖窗口像素和启动恢复。三者分开报告。原生预置设置不算设置交互验证。

## Evidence

每次执行输出独立 `target/verification/<时间>-<模式>/`。保留命令、完整日志、退出码、提交号、当时 UI 测试源码以及本轮测试产物。源码记录动作与断言，日志记录具体用例是否执行通过；失败时保留失败原因。原生另有 PNG、进程日志和 native.json；JSON 中原始截图路径可能被后续运行覆盖，以归档目录同名 PNG 为准。

证明必须同时包括动作和结果；涉及修改文件、源文件保留、进度或设置恢复时，核验用例里的磁盘读取断言，并执行对应集成检查。测试截图不能代替交互断言，内存状态不能代替持久化。未覆盖的入口、缺少夹具、未执行的模式必须报告未验证。没有 dry-run；native 会改变桌面焦点并截图。

## Cleanup

UI 与集成编排退出临时目录上下文时删除夹具和书库。原生每个窗口在 finally 中仅终止本次 Popen 进程，正常路径关闭自己的窗口并检查退出码。失败后也让编排完成清理；若外部中断，先核实本轮 PID 和临时根目录，再定向终止、删除，禁止按进程名清理。

证据目录保留。运行后检查 `exit-code.txt` 和 `transcript.log` 仍存在；native 还须有七张截图及 native.json。独立书库可共存，但截图焦点和旧测试产物路径共享：验证串行执行。helper 锁只约束 helper 调用，直接运行 tests 脚本时也须避免并发。

## Helpers

可执行脚本 `scripts/verify.py` 提供上述 doctor、ui、native、integration 四种调用，归档证据并传播失败退出码。它复用仓库编排，不另建控制接口。应用变化后使用 `/maintain-verification-skill` 更新功能地图与证据边界。
