# LazyBuddy 定向验证

主仓库只验证前端、宿主平台、通用协议、模块管理与API调用；新闻/红果业务和媒体解码测试由独立仓库负责。

| 改动 | 最小检查 |
| --- | --- |
| 根页面、设置、快捷键、窗口边界 | `cargo test -p lazybuddy --locked` |
| 清单与激活代数 | `cargo test -p lazybuddy-module-contract --locked` |
| 包安装、签名、原子更新和卸载 | `cargo test -p lazybuddy-module-manager --locked` |
| 私有服务会话 | `cargo test -p lazybuddy-service-client --locked` |
| 当前用户CLI通道 | `cargo test -p lazybuddy-host-control --locked` |
| 画面共享纹理消费者 | `cargo test -p lazybuddy-media-ipc --locked` |
| 目录组装 | `python -B scripts/verify/test-catalog-assembly.py` |

构建宿主/CLI及`cargo build --locked -p lazybuddy-service-client --example fixture`后，用`test-module-cli.py`检查离线包安装/调用/卸载；`test-host-startup.py`确认无可选运行时也能启动；`test-host-lifecycle.py`检查A→B→A、全局设置、窗口位置、隐藏及胶囊恢复，只有固定协议样例，不访问上游。

新闻仓库维护来源解析、状态保存和认证HTTP适配器检查；红果仓库维护账号/进度/封面/搜索、Go取源辅助、独立媒体及合成H.264/H.265/AAC检查。可选跨仓库集成显式提供LAZYBUDDY_HOST_DIR、LAZYBUDDY_PACKAGES_DIR和LAZYBUDDY_SDK_DIR，用已构建宿主和包体验证，不导入宿主实现。

重大跨组件变更、阶段收口与发布再执行完整相关验收，局部改动只运行相关测试。合成视频、真实账号权限、实机交互和在线发布分别报告。首个手机扫码、真实节目权限及音画同步仍需人工参与；真实写入必须独立授权。

已核实的历史资源/媒体证据见modular-architecture.md与media-runtime.md，其中旧文件位置只表示当时源码布局，不作为当前构建入口。不得cargo clean或清空target，已初始化的独立Git仓库与私有SDK均需保留。
