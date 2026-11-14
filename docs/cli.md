# LazyBuddy CLI 与构建

当前 Windows x64 开发入口为 `lazybuddy.exe` 和 `lazybuddy-cli.exe`，版本2.0.0。基础宿主不链接新闻解析或 GStreamer。旧 BriefDock 命令及构建入口已经移除；本机旧数据不迁移或删除。

## 基础构建

```powershell
cargo build --locked -p lazybuddy -p lazybuddy-cli -p lazybuddy-app-updater
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build-and-run.ps1 -BuildOnly
```

根目录 `build_and_run.bat` 构建并启动基础宿主。脚本正常退出本仓库已核对的 LazyBuddy 实例，失败不启动旧产物；不处理旧 BriefDock 或其他目录的安装。源码脚本使用本地 Cargo 缓存，首次先执行一次联网的 `cargo build --locked` 获取依赖。

生产数据在 `%LOCALAPPDATA%/LazyBuddy`。隔离实例必须同时指定唯一实例名和绝对数据目录，不得指向生产目录：

```powershell
$env:LAZYBUDDY_INSTANCE='local-preview'
$env:LAZYBUDDY_DATA_DIR=(Join-Path (Get-Location) 'target/preview-data')
.\target\debug\lazybuddy.exe --background
```

隔离模式不注册生产托盘或快捷键。`--background` 只启动后台宿主，不启动内容服务；显示当前页面后才按需工作。CLI继承同一实例环境。

## 功能包

```powershell
.\target\debug\lazybuddy-cli.exe modules list
.\target\debug\lazybuddy-cli.exe modules install-local <包含module.json的目录>
.\target\debug\lazybuddy-cli.exe modules install <ID> <HTTPS目录URL> <原始Ed25519公钥文件>
.\target\debug\lazybuddy-cli.exe modules uninstall <ID>
```

宿主运行时使用应用内功能管理；CLI拒绝直接变更运行中宿主的包，正常退出后再操作。精确依赖全部准备和校验后才原子切换 registry，失败保留旧版本；卸载最后一个使用者时回收共享运行时。清理遇到占用会报告，后续独占管理时补做，不冒充文件已经删除。

应用内目录从 EXE 同目录的 `module-catalog.json` 读取配置，字段为 `url` 和32字节数组 `public_key`。下载 `<url>` 及其 `.sig` 后验证原始JSON签名、包SHA256、大小、路径、目标平台和依赖，不接受未签名的在线目录。仓库根目录的 `module-catalog.json` 固定维护者公钥和签名目录 URL；Cargo 构建自动复制到宿主旁，基础包默认携带，不能用下载响应替换可信公钥。

## 宿主控制

`host` 接受单个JSON参数，所有 GUI/CLI 操作共用状态机。PowerShell调用时将JSON作为一个字符串参数：

```powershell
$request='{"version":1,"command":"status","args":{}}'
& .\target\debug\lazybuddy-cli.exe host $request
```

可用命令：`status`、`page.select`（id）、`settings.open`（category: page/display/system/about）、`settings.back`、`modules.open`、`modules.toggle`（打开/返回原页面）、`modules.refresh`、`modules.install/uninstall`（id）、`window.show/hide/toggle`、`app.quit`、`news.refresh`、`search`（query）、`favorites`（value）、`account.login/cancel/logout`，以及 `player.open`（series/episode）、`player.play/pause/close/seek/speed/volume/mute`。数值操作传 `value`，音量为0–1、倍速0.25–4；accepted不代表后台操作成功，随后查询状态。

`player.controls` 通过布尔value显示/隐藏覆盖控件，供CLI及隔离截图使用；正常界面由悬停、Tab和菜单状态控制。`player.favorite` 返回accepted仅表示已接收，须用status的favorite_pending/favorite确认官方读回；账号忙时明确返回account_busy。

`window.capture` 接收绝对PNG路径，使用Iced渲染器截图，不移动鼠标或激活窗口；只允许可见宿主，文件必须不存在。渲染器截图不能代替系统窗口叠加或真实解码证据。

## 独立服务与运行时

服务源码、包清单、解码运行时和专用SDK构建入口分别由LazyBuddyDev/news-service与LazyBuddyDev/hongguo-service拥有；主仓库不重编译业务服务，不从兄弟目录导入业务实现。

在对应服务仓库按照README构建与测试，得到独立ZIP/目录。开发包通过`lazybuddy-cli modules install-local <目录>`安装，正式包通过签名目录下载。`call <模块ID> <JSON>`可在不启动桌面的情况下调用服务API；version=1、id="health"、method="health"可核对健康状态。

发布维护者可从服务构建产物组装通用目录：

```powershell
python -B scripts/assemble-catalog.py --packages <新闻ZIP> <红果ZIP> <PythonZIP> <媒体ZIP> --base-url <HTTPS资源目录> --output target/catalog/catalog.json
```

该步骤读取包清单、完整依赖、文件大小和哈希，不获取或编译服务源码。之后再使用既有发布私钥签名。构建与组装不等于发布；维护者签名与发布步骤见 [模块分发](module-distribution.md)。

独立服务仍使用LazyBuddy安全命名空间。隔离真实播放、收藏写入和进度上传的开关彼此独立，具体见红果仓库API文档；默认验证不触及真实账号。

## 定向验证

按改动运行宿主对应Rust包的单元/契约测试。`cargo build -p lazybuddy-service-client --example fixture`生成通用离线协议样例，`scripts/verify/test-module-cli.py`和`test-host-lifecycle.py`验证宿主API与静默，不依赖真实业务源码。服务单元、HTTP及真实编解码验证在各自仓库运行。

临时包和截图在完成检查后清理。不得运行 `cargo clean` 或清空整个 target，里面包含私有SDK和可复用依赖。

## 基础包与签名目录

`python -B scripts/package_base.py --output target/base` 生成仅含宿主、CLI和许可说明的Windows便携包，不包含任何功能模块。`--catalog-source` 接受预先核对的可信目录配置；不能把下载响应中的公钥当成信任根。

持有既有发布私钥的维护者可在包含cryptography的私有环境运行 `python scripts/sign-catalog.py --catalog target/packages/catalog.json --key <私钥PEM路径> --source target/module-catalog.json --url <HTTPS目录URL>`。输出raw Ed25519签名和公钥配置，不生成发布身份、不打印私钥、不自动上传。私钥不放工作区，不进命令行参数值或CI日志；路径参数仅指向本机文件。

云端Windows检查自动运行定向协议和空宿主验证；Windows打包工作流仅手动触发，自有LICENSE缺失时拒绝上传分发产物。预览模块通过 `modules-v0.1.0` 发布，主程序通过 `v2.0.0-preview.1` 发布；不把未签名开发目录作为线上可信源。发布与本地构建是独立动作。

`modules.install/uninstall/refresh` 只在当前可见「功能」页且管理器空闲时接受，与GUI共用后台任务；`status` 的 `manager_busy/modules/available_modules/error` 用于核对完成，accepted不代表成功。

应用启动及每15分钟自动检查并准备更新，底部或系统设置点击「重启更新」后替换并重新启动；支持同版本测试包覆盖。`updates.check`、`updates.apply`与GUI共用入口，`status`包含update_checking/update_ready/update_applying/update_notice。更新源、签名及维护流程见[应用更新](app-updates.md)。
