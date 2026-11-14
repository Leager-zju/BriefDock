# 宿主界面技术与视觉原型

2026-11-14，候选路线为Tauri 2.12.5 + Svelte 5.57.3 + TypeScript 6.0.3；生产客户端仍为Rust/Iced。本原型用于决定是否迁移，不经更新源分发，不接入生产数据或真实账号。

## 当前结论

Web技术适合实现统一排版、响应式布局、浅深主题和视频覆盖控件；现有Rust模块协议可继续使用。但尚不满足整体替换宿主的门槛：原生D3D12共享纹理未接入网页，WebView2也引入了需要单独评估的常驻进程与内存。

因此先交付独立可运行原型供视觉比较，不自动重写正式宿主。若保留当前视频链路是前提，须先证明稳定的原生合成方案能满足覆盖控件、透明度、缩放与静默，再批准完整迁移。界面风格本身也可回用到Iced，不必等同于采用WebView。

## 原型范围

- 保留工具栏、页面区、底部信息栏；矩形窗口及控件，独立工具栏背景和分隔线。
- 新闻使用离线样例，采用高密度布局：默认780×540同屏3来源共30条，460×540同屏2来源共20条；移除大标题与说明文案，26px行高、紧凑间距、单行省略并悬停显示完整标题，提供全部/独立关注筛选；功能页保持单一垂直列表；设置区分当前功能与通用分类。
- 本地H.264合成视频经固定sample协议及单段Range提供，浏览器实际解码，控件覆盖视频；仅播放/暂停、进度、静音与音量可操作。搜索、收藏、安装/卸载和更新不进行真实业务操作。
- 前端通过Tauri调用Rust，复用crates/module-contract；没有给页面任意URL、文件或Shell执行能力。
- 页面切换按激活代数隔离迟到结果；仅首次进入或手动刷新获取离线列表，无页面轮询。隐藏/最小化暂停视频及帧回调，恢复不自行播放。各页面/区域分别保存滚动位置。

## 实测及边界

只执行原型前端3项、Rust3项单元及必要构建，未运行全项目测试。独立实际WebView检查涵盖浅深主题、功能按钮开关、460px宽设置不遮挡、1280×720实际视频帧、跳转、切页、隐藏/最小化暂停及恢复不自动播放。检查使用CDP，不移动桌面鼠标或注入物理按键；截图来自实际WebView，并已查看，不冒充整窗系统截图。

本机WebView2为Edge157，H.264及HEVC的canPlayType均返回probably；实际仅播放合成H.264，不能将HEVC能力声明写成真实HEVC/红果权限或音画同步验收。chrome.webview.getTextureStream在当前运行时脚本中为undefined；该现象与官方实验性API资料共同说明它不能作为本次已验证的生产路径，不能据此断言所有版本永远不支持。

不带调试器、从未载入视频的隐藏原型约7个进程、167.7MiB私有提交；同机隔离的现有空宿主隐藏关于页约115.6MiB私有提交、1个进程，前者多约52MiB。带CDP连接、播放过样例的原型约8个进程、约255–270MiB私有提交。这些是密度调整前同技术原型的短时采样；页面、媒体缓存、窗口尺寸与调试条件不同，不作为严格性能倍数。无调试采样自身隐藏窗口收尾未成功投递退出消息，已核对并结束其专属PID；它不代表产品正常退出失败，独立实际WebView检查的正常关闭已通过。各自短时CPU采样及不带调试器的补充数值保存在target/verification-shell；工作集按进程相加会重复计算共享页，不能当作实际独占物理内存。短时零CPU不能证明长期零开销，暂停视频/停止帧回调也不等于WebView内部线程全部停止。

验证文件：target/verification-shell/desktop.json、native-baseline.json、clean-shell.json及浅深主题/视频/窄设置截图。它们是本机证据；跨端接手应重新执行需要核对的原型检查，不复用本机PID或内存数值。

## 构建与打开

本地已构建原型位于target/shell-prototype/lazybuddy-shell-prototype.exe，与sample.mp4放在同一目录即可直接双击。使用单独的应用标识和WebView配置目录，不与LazyBuddy共享登录、设置或更新记录。需要系统已安装的WebView2；本轮未验证缺少WebView2机器的首次安装。

从源码构建并打开：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-shell-prototype.ps1
```

只构建使用-BuildOnly。需要Node24、Rust/MSVC与系统WebView2；合成片段使用本项目已准备的私有GStreamer SDK生成，不打包SDK，不读取红果片源。

定向单元：

```powershell
npm --prefix apps/shell-prototype ci --legacy-peer-deps --no-audit --no-fund
npm --prefix apps/shell-prototype run check
npm --prefix apps/shell-prototype test
npm --prefix apps/shell-prototype run build
cargo test --locked --manifest-path apps/shell-prototype/src-tauri/Cargo.toml
node apps/shell-prototype/tests/desktop.mjs target/shell-prototype/lazybuddy-shell-prototype.exe
```

实际WebView检查是显式执行项，不默认混入单元CI。CI只在原型发生变化时执行其类型检查、单元与必要构建。原型是独立Cargo工作区，生产工作区和安装包不增加Tauri依赖。

## 调研来源

- [Tauri进程模型](https://v2.tauri.app/concept/process-model/)：Rust核心与系统WebView分进程。
- [WebView2合成接口概览](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/overview-features-apis)：原生合成与普通窗口托管的接口不同。
- [ExperimentalTextureStream](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2experimentaltexturestream?view=webview2-1.0.4071-prerelease)：资料明确标为预发布SDK的实验性接口；本原型未依赖它，也未将D3D12帧逐张转为图片传给网页。

后续如决定采用Web前端，应将“原生媒体合成闭环”和“可接受的宿主资源预算”作为同一界面升级任务的迁移门槛，保留现有模块服务、签名更新、数据隔离与静默约束。
