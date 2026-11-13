# 验证入口

运行与证据归档使用 [verify-reader](../.agents/skills/verify-reader/SKILL.md)。功能旅程维护在其 [功能地图](../.agents/skills/verify-reader/features/README.md)，这里不重复覆盖清单。

需要直接使用测试编排时，从仓库根目录运行：

```sh
python3 tests/run.py --help
python3 tests/run.py
```

具体参数、构建步骤和失败处理见 [run.py](run.py)；夹具见 [support.py](support.py)。界面编排入口为 [test_production.py](ui/test_production.py)，真实事件与断言在 [src/ui_tests.rs](../src/ui_tests.rs)。原生桌面前提与进程清理见 [native_smoke.py](native_smoke.py)。

判断证据时区分事件交互、磁盘副作用和实际窗口渲染。测试通过只证明所执行的路径；预置状态的原生截图不证明生成该状态的用户操作。缺少环境时报告未验证及原因。
