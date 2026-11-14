# 宿主界面技术与视觉原型

2026-11-14，候选路线为Tauri 2.12.5 + Svelte 5.57.3 + TypeScript 6.0.3；生产客户端仍为Rust/Iced。本原型用于决定是否迁移，不经更新源分发，不接入生产数据或真实账号。

## 当前结论

Web技术适合实现统一排版、响应式布局、浅深主题和视频覆盖控件；现有Rust模块协议可继续使用。但尚不满足整体替换宿主的门槛：原生D3D12共享纹理未接入网页，WebView2也引入了需要单独评估的常驻进程与内存。

因此先交付独立可运行原型供视觉比较，不自动重写正式宿主。若保留当前视频链路是前提，须先证明稳定的原生合成方案能满足覆盖控件、透明度、缩放与静默，再批准完整迁移。界面风格本身也可回用到Iced，不必等同于采用WebView。

## 原型范围

- 保留工具栏、页面区、底部信息栏；矩形窗口及控件，独立工具栏背景和分隔线。
- 新闻使用离线样例，采用高密度布局：默认780×540同屏3来源共30条，460×540同屏2来源共20条；移除页面大标题与说明文案，26px行高、紧凑间距、单行省略并悬停显示完整标题，提供全部/独立关注筛选；功能页保持单一垂直列表；设置区分当前功能与通用分类。
- 本地H.264合成视频经固定sample协议及单段Range提供，浏览器实际解码，控件覆盖视频；内容区只保留搜索、收藏及播放：搜索与收藏使用内存样例，进入播放后仅显示视频、返回、收藏及播放/进度/静音/音量控件；技术标签和说明全部移到底部。安装/卸载和更新不进行真实业务操作，样例收藏不代表红果账号收藏。
- 前端通过Tauri调用Rust，复用crates/module-contract；没有给页面任意URL、文件或Shell执行能力。
- 页面切换按激活代数隔离迟到结果；仅首次进入或手动刷新获取离线列表，无页面轮询。隐藏/最小化暂停视频及帧回调，恢复不自行播放。各页面/区域分别保存滚动位置。

## 工具栏、返回与贴边收起

功能/设置全宽度下均只显示图标，保留可访问名称和提示。功能页标题右侧返回与工具栏功能图标共用导航，恢复打开前的内容或设置分类。

原型沿用现有宿主WM_MOVING/WM_EXITSIZEMOVE和当前显示器工作区算法，独立适配Tauri，不导入Iced实现。只有拖动完成且靠近左右边缘才收起，单纯缩放不触发。收起为40逻辑像素圆形Logo，当前DPI下换算物理尺寸，用原生椭圆区域裁剪显示及命中；收起期间浮在上层，展开恢复原有置顶状态、有效位置、尺寸、页面及滚动位置。Logo点击展开，悬停不自动展开。

窗口Logo可见不代表内容页激活：模型按收起状态拒绝内容请求，前端暂停视频与帧回调；展开不自动播放。Tauri主窗口显式声明core:event:allow-listen及allow-unlisten，等待订阅就绪，不把订阅拒绝静默掩盖为已连接。

仅新增导航及贴边的实际检查：`node apps/shell-prototype/tests/dock.mjs target/shell-prototype/lazybuddy-shell-prototype.exe`。它通过私有测试进程的窗口消息验证左右拖动、圆形区域/角落命中、Logo恢复、设置滚动保留及视频静默，不移动桌面鼠标，不触碰生产实例。证据为target/verification-shell/dock.json及对应截图。几何单元覆盖负坐标、工作区边界与DPI缩放；本机没有进行多显示器/跨DPI真实拖动验收。

## 实测及边界

初版执行原型前端3项、Rust3项单元及必要构建；新增返回导航和贴边几何后为前端4项、Rust6项。只按变更执行对应单元与必要构建，未运行全项目测试。独立实际WebView检查涵盖浅深主题、功能按钮开关、460px宽设置不遮挡、1280×720实际视频帧、跳转、切页、隐藏/最小化暂停及恢复不自动播放，内存样例搜索/收藏、从收藏打开播放和返回列表。新三视图媒体回归使用调试构建，最终优化构建同步相同逻辑并补验紧凑新闻；它不能代替原生播放器或真实节目验收。检查使用CDP，不移动桌面鼠标或注入物理按键；截图来自实际WebView，并已查看，不冒充整窗系统截图。

本机WebView2为Edge157，H.264及HEVC的canPlayType均返回probably；实际仅播放合成H.264，不能将HEVC能力声明写成真实HEVC/红果权限或音画同步验收。chrome.webview.getTextureStream在当前运行时脚本中为undefined；该现象与官方实验性API资料共同说明它不能作为本次已验证的生产路径，不能据此断言所有版本永远不支持。

不带调试器、从未载入视频的隐藏原型约7个进程、167.7MiB私有提交；同机隔离的现有空宿主隐藏关于页约115.6MiB私有提交、1个进程，前者多约52MiB。带CDP连接、播放过样例的原型约8个进程、约255–270MiB私有提交。这些是密度调整前同技术原型的短时采样；页面、媒体缓存、窗口尺寸与调试条件不同，不作为严格性能倍数。无调试采样自身隐藏窗口收尾未成功投递退出消息，已核对并结束其专属PID；它不代表产品正常退出失败，独立实际WebView检查的正常关闭已通过。各自短时CPU采样及不带调试器的补充数值保存在target/verification-shell；工作集按进程相加会重复计算共享页，不能当作实际独占物理内存。短时零CPU不能证明长期零开销，暂停视频/停止帧回调也不等于WebView内部线程全部停止。

验证文件：target/verification-shell/desktop.json、native-baseline.json、clean-shell.json及浅深主题/视频/窄设置截图。它们是本机证据；跨端接手应重新执行需要核对的原型检查，不复用本机PID或内存数值。

## 构建与打开

用户反馈后的版本已去除页面大标题、英文标签、技术说明和宣传文案；播放页直接填满可用页面区。

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

用户已明确：页面区只保留功能必需内容，不自行添加装饰标题、宣传语或说明卡片，除非用户/开发者明确提出；具体可执行规范见AGENTS.md。

后续如决定采用Web前端，应将“原生媒体合成闭环”和“可接受的宿主资源预算”作为同一界面升级任务的迁移门槛，保留现有模块服务、签名更新、数据隔离与静默约束。
