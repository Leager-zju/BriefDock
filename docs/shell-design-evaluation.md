# 宿主界面技术与视觉原型

2026-11-14，候选路线为Tauri 2.12.5 + Svelte 5.57.3 + TypeScript 6.0.3；生产客户端仍为Rust/Iced。本原型用于决定是否迁移，不经更新源分发；显式选择真实服务包后可扫码验收，凭据和数据与生产应用隔离。

## 当前结论

最新方向：服务提供受控、可供浏览器读取的媒体资源，前端使用HTML video解码、同步及绘制控件，复用现有私有Range MP4输入层。资源API、账号/搜索/收藏/选集入口及独立WebView合成媒体闭环已经接通；真实红果输入仍待用户扫码验收。原生D3D12合成实验保留在codex/lazybuddy-native-web-video，不合入新的默认路线。

Web技术适合实现统一排版、响应式布局、浅深主题和视频覆盖控件；现有Rust模块协议继续使用。整体替换仍待真实业务入口接线、节目验收及完整资源预算确认；WebView2增加的常驻进程与内存需要单独评估。

当前生产宿主和签名目录不变，不将候选服务或原型经自动更新投递给旧Iced客户端。界面迁移保持模块API、数据隔离及静默约束。

## 浏览器资源闭环

独立服务增加playback.open/status/suspend/resume/checkpoint/close。open接收剧ID、集号、清晰度和明确Origin；完成账号/权限/媒体输入准备后返回随机本地Range MP4会话资源，不启动GStreamer。前端拿到的是受控本地资源URL及目录/档位/源尺寸；实际解码尺寸、播放时间和帧媒体时间由HTML video提供。Cookie、密钥和上游签名地址留在服务，随机本地URL同样不记录日志或公开状态。

Server核对Host、随机路径和唯一允许Origin，提供CORS、GET/HEAD/单段Range及预检。隐藏先卸载前端src，再暂停服务读取；旧代在途数据不再填充缓存或派生新请求，恢复同一资源和位置且不自播。设置/切页释放所属服务与媒体资源，关闭等待正常收尾。checkpoint及生命周期控制核对会话代数。现有JSON-lines业务协议不变；完全独立网站仍需部署API后端/本地代理，不能直接操作桌面子进程。

本机实际Edge157 WebView已验证外部样例服务包输出的640×360 H.264与H.265/AAC，暂停seek的requestVideoFrameCallback.mediaTime=3.1秒，隐藏src卸载/资源请求409，恢复3.1秒暂停，进入设置后旧URL失效，正常退出完成；实际截图已查看。服务样例只有Python依赖，不启动GStreamer。37项受影响服务单元和新增Rust桥接白名单单元通过；未运行全工作区回归。AAC静音，无真实账号、处理后的真实节目输入或音画同步结论。

候选hongguo0.2.0已独立组装，只有python-runtime0.1.0依赖；默认打包不包含media-runtime。真实候选入口health通过，无GStreamer目录。未改动的搜索辅助从既有发布包复用（SHA256 b5adbc861409f7dc1b6f355ffb22e6efbf0b623e70e93e87935babb53c1c891b），本轮没有重编译或跑搜索/Go测试。现有Iced消费者不应安装这套候选目录，待新宿主完整接入后再发布。

显式LAZYBUDDY_BROWSER_PACKAGES包目录开启完整服务入口：未登录显示登录按钮、弹层渲染二维码；登录后搜索与账号收藏使用实际服务数据，播放使用选中剧集ID，提供目录选集、清晰度和本地观看断点。默认不传包仍为离线视觉样例。开发样例入口由红果服务仓库自己的scripts/verify/stage-browser-fixture.py组装，不放入正式包；宿主只读清单/调用子进程API，不跨仓库导入实现。

真实网络手动验收入口（在项目根目录执行）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run-shell-prototype.ps1 -ServicePackages .\target\service-exports\lazybuddy-hongguo-service\target\packages-browser-current
```

先从托盘菜单退出同一入口的旧原型（新版本的“×”只隐藏）。脚本调用npm.cmd、打印两阶段构建进度；服务模式不需要GStreamer。当前候选hongguo0.2.1新增凭据作用域，宿主拒绝低于0.2.1的包，使用shell-preview命名空间安全保存登录及设备档案；本地进度在%LOCALAPPDATA%/LazyBuddyShellPreview/hongguo。搜索与用户明确点击的收藏操作允许真实网络；云端进度上传默认关闭。切换到全局设置、功能页或隐藏窗口停止扫码任务。不得把停止扫码等同于退出已保存账号。

验证脚本apps/shell-prototype/tests/browser-media.mjs使用模拟账号，已验证二维码显示/取消、全局设置终止扫码、登录搜索播放、退出账号、功能页返回内容、暂停seek及隐藏恢复3.1秒/1.5倍速。前端6项、Rust播放桥3项、服务33项受影响单元通过。证据target/verification-browser-video/browser-media.json；没有使用真实账号、真实扫码或上游节目，因此真实网络/权限/音画同步留给用户手动验收。

复现时先在独立服务仓库构建包，再用其stage-browser-fixture.py的--runtime-packages显式选择生成包目录。原型侧设置LAZYBUDDY_BROWSER_PACKAGES为样例包目录、LAZYBUDDY_BROWSER_FIXTURE_FILE为至少4秒且不超过8MB的自制合成MP4；构建后运行`node apps/shell-prototype/tests/browser-media.mjs <候选exe绝对路径>`。驱动只关闭自身实例和临时WebView配置；固定样例入口不放进正式服务包。代码CI：宿主e0e950d运行43587778472、服务554ab6a运行43587786151均成功。源码仅在独立分支，不通过更新源覆盖生产。

## 历史原生视频实验（独立分支）

以下实现及原生命令只对应codex/lazybuddy-native-web-video（6a7d88a），不适用于当前浏览器路线分支。服务暂停帧实验在codex/lazybuddy-paused-video（a1e665d）。初轮暂停preroll缺口后来在服务子类中补齐并以红→绿实际像素验证；最后新增的EOS后重播回归仍超时，未宣称完整原生播放器交付。这些实验不发布为默认模块。

原生候选使用现有独立media-runtime包及service-client。GStreamer仍在所属Windows Job的外部进程解码；media-ipc接收D3D12共享句柄与fence，按生产端LUID选择相同适配器，GPU复制到宿主保留纹理，完成后才确认生产端释放。wgpu27将该纹理绘入DirectComposition交换链，放在透明WebView子窗口下；Svelte只画覆盖控件。正常路径无逐帧JavaScript像素、CPU图片或GStreamer链接，也不依赖ExperimentalTextureStream。

独立媒体线程处理取帧、复制、呈现及控制，UI线程不等待GPU。隐藏保留暂停管线及最后帧，展开只重绘，不自播；设置或切根页面释放自身解码进程。显式播放期间在帧事件外最多每250ms采样一次真实状态以接收末帧之后的EOS，暂停、隐藏、离页和EOS后无该采样。缩放只改合成表面及视觉位置。

2026-11-14实测：1280×720 H.264与640×360 H.265/AAC合成MP4均经已发布media-runtime真实解码并显示。检查连续帧、暂停冻结、跳转后继续、1.5倍速推进、0/0.5/1透明度的预乘RGBA、缩放保持同一PID与打开次数、贴边暂停/恢复不自播、设置释放进程与返回暂停，以及真实EOS后停止。PrintWindow整窗截图包含原生画面和HTML覆盖控件，已实际查看；不以CDP截图替代合成证据。AAC保持静音，本轮没有证明音频输出质量、音高或真实节目音画同步。

初始化曾出现只枚举到软件适配器：接收端先建立D3D12设备，再由调试构建的wgpu启用原生调试层，会使已有设备失效。按[微软EnableDebugLayer约束](https://learn.microsoft.com/en-us/windows/win32/api/d3d12sdklayers/nf-d3d12sdklayers-id3d12debug-enabledebuglayer)，本消费者不在既有设备之后启用原生调试层，仍保留wgpu自身验证。未采用软件适配器回退。

已知未完成边界：GStreamer1.26.11的[d3d12ipcsink实现](https://github.com/GStreamer/gstreamer/blob/1.26.11/subprojects/gst-plugins-bad/sys/d3d12/gstd3d12ipcsink.cpp)仅发送render帧，未发送暂停preroll。暂停seek可完成管线跳转，但保留旧画面，继续后才得到新帧；不能把position返回成功写成暂停画面已刷新。IPC名为pts的字段实际是系统呈现时钟，候选状态分别报告frame_timestamp_ns和管线position，没有宣称取得实际媒体帧PTS。正式切换前须在独立媒体服务中完善暂停帧与可核对的媒体PTS/seek完成元数据，不能在宿主悄悄播放或修改静音来获取一帧。

原生入口仅显式本地合成模式启用，不替换普通浏览器样例或生产宿主，不允许失败后回退HTML视频。设置LAZYBUDDY_NATIVE_FIXTURE=1、LAZYBUDDY_NATIVE_FIXTURE_URL=http://127.0.0.1:<port>/sample.mp4，以及LAZYBUDDY_NATIVE_PACKAGES为显式包目录；后者只读取media-runtime与python-runtime清单/入口，不导入另一个仓库源码。测试驱动自建该固定Range资源并关闭自身进程，不读取真实账号。

```powershell
$env:CARGO_TARGET_DIR="$PWD/target/qa-native-video/build"
npm --prefix apps/shell-prototype run build
cargo build --locked --manifest-path apps/shell-prototype/src-tauri/Cargo.toml
cargo test --locked --manifest-path apps/shell-prototype/src-tauri/Cargo.toml native::tests
cargo test --locked -p lazybuddy-media-ipc --features composition composition::tests
node apps/shell-prototype/tests/native-media.mjs target/qa-native-video/build/debug/lazybuddy-shell-prototype.exe
```

驱动默认本机target/service-exports/lazybuddy-hongguo-service/target/packages-current，可传入短小合成MP4作为第三个命令行参数；包不存在时应先准备明确的运行时包，不能以本机服务源码替代。证据在target/verification-native-video的h264.json、h265-aac.json与对应整窗截图。当前验证为调试构建，未作优化版资源预算、跨DPI/显示器、设备丢失或真实节目验收。

## 原型范围

- 保留工具栏、页面区、底部信息栏；矩形窗口及控件，独立工具栏背景和分隔线。
- 新闻使用离线样例，采用高密度布局：默认780×540同屏3来源共30条，460×540同屏2来源共20条；移除页面大标题与说明文案，26px行高、紧凑间距、单行省略并悬停显示完整标题，提供全部/独立关注筛选；功能页保持单一垂直列表；设置区分当前功能与通用分类。
- 本地H.264合成视频经固定sample协议及单段Range提供，浏览器实际解码，控件覆盖视频；内容区只保留搜索、收藏及播放：搜索与收藏使用内存样例，进入播放后仅显示视频、返回、收藏及播放/进度/静音/音量控件；技术标签和说明全部移到底部。安装/卸载和更新不进行真实业务操作，样例收藏不代表红果账号收藏。
- 前端通过Tauri调用Rust，复用crates/module-contract；没有给页面任意URL、文件或Shell执行能力。
- 页面切换按激活代数隔离迟到结果；仅首次进入或手动刷新获取离线列表，无页面轮询。隐藏/最小化暂停视频及帧回调，恢复不自行播放。各页面/区域分别保存滚动位置。

## 工具栏、返回与贴边收起

功能/设置全宽度下均只显示图标，保留可访问名称和提示。两者是平级页面；功能页标题右侧返回与工具栏功能图标共用导航，始终返回当前根页面内容。设置→功能→返回也回根页面，不恢复设置。

窗口“×”及系统CloseRequested只隐藏到托盘；左键托盘Logo或右键“显示 LazyBuddy”恢复，右键“退出”才执行服务收尾并退出进程。明确CLI quit与close分开，验证脚本收尾使用quit。通过Tauri内置tray-icon能力创建图标，复用应用Logo及既有页面显隐生命周期。

Windows无焦点显示原先直接ShowWindow，Tao内部可见标志仍为初始hidden，随后框架hide被判成无变化，导致仅页面静默而窗口仍可见。现显/隐/最小化成对使用原生窗口路径，保留无焦点恢复。tests/tray.mjs通过Windows实际可见性、原生WM_CLOSE、托盘库真实通知回调和明确quit验证，而非只看前端visible。Chrome_WidgetWin_0注销警告的原因未确认；关闭到托盘路径不销毁WebView，本次该路径未出现警告，不承诺Chromium在真正退出时永不输出此日志。

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
