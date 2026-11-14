# LazyBuddy

<img src="assets/lazybuddy.svg" width="96" height="96" alt="LazyBuddy">

上班摸鱼的桌面搭子。Windows x64 主程序负责界面和窗口，新闻、红果短剧及以后新增的功能按需安装；未使用的功能停止工作，卸载后回收无引用的运行时。

**2.0 正在重构，尚未发布稳定版本。** 基础宿主、独立服务、模块安装事务和媒体共享纹理已有隔离验证；视频内控件、窗口贴边和隔离生命周期已迁移；预览版已提供签名模块目录，真实账号的新入口仍待验收。范围与证据见 [TASKS.md](TASKS.md)。旧 BriefDock 不再作为开发入口，不提供旧数据迁移。

## 下载使用

下载 [Windows x64 预览版](https://github.com/LazyBuddyDev/LazyBuddy/releases/tag/v2.0.0-preview.1)，解压后运行 `lazybuddy.exe`。在顶部「功能」中安装新闻或红果，再从「选择功能」进入；红果的两个运行时自动安装。不使用的功能可直接卸载。保留 EXE 同目录的可信源配置，无需安装 Python、Go 或 GStreamer。启动后自动检查并准备应用更新，点击「重启更新」即可应用；测试包即使版本号相同也能识别覆盖更新，见[应用更新](docs/app-updates.md)。

## 本地开发

需要 Windows x64、Rust/MSVC 工具链和 Windows SDK。首次获取 Cargo 依赖后：

```powershell
cargo build --locked -p lazybuddy -p lazybuddy-cli -p lazybuddy-app-updater
.\build_and_run.bat
```

批处理只构建基础宿主，不要求预装 Python、Go 或 GStreamer。只构建、不启动：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build-and-run.ps1 -BuildOnly
```

功能模块由组织内独立仓库构建和发布：

- [news-service](https://github.com/LazyBuddyDev/news-service)：新闻API、偏好和缓存。
- [hongguo-service](https://github.com/LazyBuddyDev/hongguo-service)：账号、搜索、收藏、进度、封面、取源及媒体运行时。

本仓库不包含业务服务源码、解码运行时、真实服务测试或其SDK准备脚本。宿主与CLI只通过版本化API调用已安装包；离线验证使用通用协议样例。预览版随包提供可信目录配置，可在应用内安装、卸载功能，无需重启。签名与发布维护见 [模块分发](docs/module-distribution.md)。组织归属为LazyBuddyDev，自有代码采用MIT；真实账号验收仍待完成。

独立服务的构建、测试与包体准备见各仓库README。运行`lazybuddy-cli modules install-local <目录>`可显式安装开发包，须先正常退出宿主。真实账号凭据只由服务保存在Windows安全存储中。

## 文件结构

| 目录 | 职责 |
| --- | --- |
| `apps/desktop` | Iced 界面、页面调度、窗口与通用设置 |
| `apps/cli` | 模块管理、宿主控制和独立服务调用 |
| `crates/module-contract` | 功能清单、依赖、页面描述和生命周期代数 |
| `crates/module-manager` | 签名目录、包校验、原子安装更新及卸载 |
| `crates/service-client` / `host-control` | 私有服务会话及当前用户 CLI 通道 |
| `crates/media-ipc` | 无 GStreamer 链接的共享纹理消费者 |
| `contracts` | 通用协议的离线验证样例 |
| `scripts` | 宿主构建、签名目录组装和隔离验证 |

日常运行相关单元/契约测试；合成媒体、真实接口和界面验收分别记录，避免每次局部改动重复全量验收。外部接入见 [模块 API](docs/module-api.md)。设计和执行清单见 [模块化架构](docs/modular-architecture.md)，媒体验证边界见 [独立媒体运行时](docs/media-runtime.md)，长期规范见 [AGENTS.md](AGENTS.md)。

自有代码采用[MIT](LICENSE)，Copyright (c) 2026 Leager；第三方代码保留原许可。预览版不代表真实账号或所有剧集权限已经验收。
