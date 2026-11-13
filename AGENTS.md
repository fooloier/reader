# 协作约定

默认使用简体中文。代码沿用现有风格，默认不写注释。提交信息使用 Conventional Commits。

代码是当前行为的唯一事实来源。先从 [源码导航](docs/architecture.md) 定位实现和测试，再修改；历史讨论只能解释动机，不能覆盖当前代码。

按任务加载资料：

- 修改 GPUI 代码：读 [.agents/skills/gpui-kit/SKILL.md](.agents/skills/gpui-kit/SKILL.md)。
- 改变界面或交互设计：另读 [.agents/skills/gpui-kit-design-guides/SKILL.md](.agents/skills/gpui-kit-design-guides/SKILL.md)。
- 验证用户行为：用 [.agents/skills/verify-reader/SKILL.md](.agents/skills/verify-reader/SKILL.md)。
- 需要理解范围或历史取舍：读 [决策背景](docs/decisions.md)。

文档只承载用户使用说明、任务导航、验证操作和代码无法表达的决策背景。类型字段、默认值、算法、完整接口、测试数量和覆盖清单留在代码与测试中。新增资料应接入现有导航，避免新增平行说明。改变用户操作时更新 README；改变验证入口时更新对应技能；改变决策时记录新依据及被替代的决定。
