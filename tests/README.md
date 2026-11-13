# Reader 测试参考

所有编排和夹具生成脚本都是 Python 标准库 `.py`，不需要 pip 包。PDF 夹具手工生成对象与 xref；EPUB 夹具使用 zipfile 和 XHTML；PNG 使用 struct 与 zlib。

## 命令

命令在项目根目录执行。

```text
python3 tests/run.py
python3 tests/run.py unit integration
python3 tests/run.py ui --skip-build
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --features test-support -- -D warnings
python3 tests/native_smoke.py
```

`run.py` 默认先编译工作区及 test-support，再执行全部层。`--skip-build` 要求已存在对应二进制；UI 层仍由 cargo test 编译 Rust 测试。未知层、缺少测试层、零测试、超时和失败均返回非零状态。

## 分层边界

| 层 | 验证对象 | 内容 |
| --- | --- | --- |
| unit | 独立 Rust core 与 runtime 单元目标、路径解析 | 标签身份、格式设置、位置类型、XDG 绝对路径与默认值、PDF 字符串解码、EPUB 内联文本 |
| integration | JSONL reader-driver 调用真实 Library | 递归导入、软链接循环、去重、损坏与不支持文件、元数据与封面、重命名与删除、源文件完整性、进程重启、孤立原件保留、缺失记录恢复、数据库重置、损坏 JSON 备份与默认值、真实导入中断、封面重建、UTF-8 文件名字节上限、格式设置 |
| ui | 生产 ReaderApp、GPUI 组件与真实平台文本系统 | 真实点击、键盘、输入、拖选、滚轮、书架布局与主题、标签去重、对话框、面板与焦点、PDF 和 EPUB 复制、书签与正文内部链接跳转、层级与无效目标隐藏、当前书签选中与重开滚动定位、空书签、空章节、键盘与工具栏首尾导航、浮层和输入框拦截、特殊字符锚点重开及启动恢复、重命名失败后进度、文件夹导入统计与失败反馈、打开书籍删除、退出失败重试与排队保存 |

当前 Python 共 17 个测试，包含 3 个 unit、13 个 integration 和 1 个 UI 编排测试；其中 Rust core 运行 2 项，runtime 运行 7 项，应用库运行 2 项书签定位单元测试和 14 项生产 UI 测试。Rust 测试摘要按完整行提取并比较总数，不能用 `12 passed` 匹配 `2 passed`。Rust UI 测试使用 TestWindowExt 操作生产视图，点击不会被替换成业务函数调用。书架响应式布局测试使用 7 本长标题书籍，检查 160–1500 像素窗口和 16、20 像素基准字号下的列宽、间距、末行对齐与标题边界。无图形会话下的布局事件测试不能证明窗口实际像素。

`reader-driver` 每行接收一个 JSON 请求，每行输出一个包含 `ok` 和 `value` 或 `error` 的响应。其第一个参数必须为隔离根目录，只有显式 `--native` 才使用原生路径。集成测试每次请求使用新进程读取磁盘结果，测试数据全部放在临时目录。

## 原生检查

`native_smoke.py` 需要可用的 Wayland 会话、当前 Hyprland 的 Lua dispatch 接口、hyprctl、grim，以及带 test-support 编译的 reader。缺少前提时直接失败，不记为跳过或通过。

原生脚本使用临时 XDG 根目录并启动真正的应用进程，依次检查浅色卡片、深色列表、EPUB 章节开头、EPUB 已保存位置、PDF、真实 XHTML 的 SVG 图片封面和特殊字符锚点，共 7 个场景。它等待窗口映射、正文就绪和连续三张相同截图，再关闭自己的窗口。退出必须成功，读取数据库验证 EPUB 段落、PDF 页码和未激活 PDF 标签的位置没有被覆盖。

慢队列退出回归在生产 UI 中阻塞真实服务线程超过 200 ms，验证 Ctrl+Q 不提前退出，释放任务后检查最新位置、主题和工作区落盘。另一个 UI 回归制造设置写入失败，验证应用保持打开并允许重试。原生窗口关闭使用同一保存方法，但没有人为插入慢任务。

真实 EPUB 夹具分别验证 EPUB 2 NCX 和 EPUB 3 nav，含 XML 声明、外部 DOCTYPE 标识、`&nbsp;`、SVG 包装、空章节、未使用的竖排 CSS、混合内联容器，以及含中文、空格、字面 `%20`、井号和括号的资源。Markdown 转换用实际 Markdown 渲染器验证文字和图片属性，生产 UI 还实际点击内部链接。

脚本只聚焦和关闭它启动的 PID。当前原生检查没有自动操作操作系统文件选择器，也没有实测 macOS、Windows、屏幕阅读器、所有字体或复杂出版物。用户点击流程由生产 UI 测试覆盖，原生检查用于像素和生命周期证据，两者分别报告。

## 产物

| 文件 | 内容 |
| --- | --- |
| target/test-results/layers.log | 最近一次 Python 分层测试结果 |
| target/test-results/ui.log | Rust 生产 UI 测试输出 |
| target/test-results/native.json | 原生窗口尺寸、截图路径、退出码 |
| target/test-results/native-*.png | 真实窗口截图 |
| target/test-results/native-*.log | 原生进程输出 |

这些文件位于 Git 忽略的 target 目录。应用数据和夹具临时目录在测试完成后删除，不写入用户真实书库。
