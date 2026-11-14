# LazyBuddy 独立媒体运行时

状态：2026-11-14，合成媒体验证通过，真实账号与完整音画同步验收待完成。

宿主不链接 GStreamer。红果服务使用私有取源进程提供受限回环资源，媒体运行时独立解码，通过固定 GStreamer 1.26.11 的 D3D12 IPC 传递共享纹理；宿主后台等待解码 fence，复制到自身纹理后才确认生产端释放。控制消息不包含原始帧、密钥或真实签名地址。

媒体加载异步返回 loading；换集取消旧取源及解码进程，源结果绑定当前代数。页面离开释放媒体资源，隐藏保留已加载的暂停管线；恢复不自行播放。账号最终进度事务独立于播放中的本地检查点，不以云端网络锁住本地记录。收藏读写返回 pending，媒体控制仍可响应。

## 已执行的验证

- 三帧共享纹理实际像素与 CPU 参考一致；生产 Iced Primitive 离屏读回通过两种目标格式和 0/50/100% 透明度。
- 私有 Python/GStreamer 开发包脱离开发 PATH 运行；合成媒体暂停、实际跳转位置、真实 EOS 和加载中换集取消通过。未使用真实账号。
- `scripts/verify/test-host-media.py` 已通过完整新宿主链路：假账号、本机MP4、独立代理/解码、GPU共享纹理及Iced截图检出196544个视频像素；保持宽高比，隐藏期间保留暂停的同一组辅助进程且位置不推进，设置页释放进程，返回后明确继续才重建。所选1.5倍速、临时2倍速与暂停还原均核对，焦点保持不变；未执行物理键盘注入。
- `scripts/verify/test-media-audio.py` 使用生产 Player 控制及音频链，仅将最终设备替换成 appsink，直接提取合成 PCM，不使用扬声器。8秒、440Hz 输入在1/1.5/2倍速分别输出8.00/5.31/3.99秒，频率439.86/439.91/439.80Hz。真实节目音画同步尚未验收，不能由音频时长或纹理单项成功外推。

音频链沿用 `audioconvert ! scaletempo ! audioresample ! autoaudiosink`；保音高原理及变速 seek 行为参考 [GStreamer scaletempo 官方文档](https://gstreamer.freedesktop.org/documentation/audiofx/scaletempo.html)。正向 seek 使用 SET 开始位置、NONE 结束位置，避免截短播放段。

## 样例选择的限制

最初的 HTTP push-mode AVI 样例中，avidemux 收到 rate=2 的 seek 后，在字节重定位完成时下发 rate=1 的时间段，实际 PCM 和墙钟均保持8秒。该结果不能归因到生产 MP4 的音频链。改用 qtmux/qtdemux 路径后，scaletempo 实际收到1.5/2的速率，输出时长和频率符合预期。AVI 仍用于有限的纹理/取消/基本位置检查，不用于证明变速性能。

当前测试没有公开新的真实媒体权限、未执行新命名空间的首次扫码，也未上传实际观看进度或修改官方账号收藏。

首次加载的解码管线在preroll前不能发起seek；当前默认倍速无变化时不seek，非默认倍速/跳转等待有界preroll。该缺陷由完整宿主验证发现，已有针对性单测。顶点阶段使用宽高比uniform时，绑定可见性必须同时包含顶点与片元；六组GPU验证同时核对中心颜色和留边透明。

## 播放包精简

2026-11-14：媒体包从全量GStreamer运行库改为固定播放插件及DUMPBIN导入DLL闭包。包内保留playback/typefind/MP4/解析器、D3D11/12、libav软件回退、音频转换/scaletempo/输出、私有HTTP输入；AVI/JPEG/app支持同生产管线验证。完整文件清单随包保存为runtime/playback-files.json。未使用的编码器与传输插件不分发，SDK保留供构建和调研。

候选压缩包107249316→19367561字节。清空开发PATH、使用全新插件缓存后，真实宿主合成MP4画面、覆盖控件、隐藏/设置/退出生命周期通过；独立代理的3帧、实际跳转、EOS与换集取消通过。精简包PCM仍为1/1.5/2倍速8.00/5.31/3.99秒，约440Hz。测试探针曾早于代理管线preroll连接，改为等待实际loaded后通过；宿主消费者原本已有有界连接等待。

DLL闭包使用微软官方[DUMPBIN /DEPENDENTS](https://learn.microsoft.com/en-us/cpp/build/reference/dependents?view=msvc-170)，不手写PE解析器。显式动态加载的GStreamer入口和插件另列为根；因此不能仅凭导入闭包宣称所有动态能力均已覆盖。真实节目编码、权限与音画同步仍待首次扫码后验收，不外推合成样例结论。

额外编码验证：精简开发包以H.264和H.265合成MP4分别通过3帧像素对照、暂停、真实跳转及EOS；使用`test-media-service.py --packaged --codec h264|h265`。AAC合成MP4经精简包解码后的PCM在1/1.5/2倍速为8.021/5.34/3.99秒，频率440.09/440.09/440.13Hz，`test-media-audio.py --aac`不使用扬声器。首次AAC样例因S16LE不符合编码器F32LE输入而失败，增加样例编码前audioconvert后通过；这与产品解码链无关。上述均为本机合成编码，不证明真实权限或真实节目音画同步。

实际尺寸状态：控制进程通过[GStreamer当前协商caps](https://gstreamer.freedesktop.org/documentation/gstreamer/gstpad.html#gst_pad_get_current_caps)读取输出pad的宽高，持有的caps引用每次释放；不以模型声明尺寸冒充实际输出。宿主分别返回width/height与decoded_width/decoded_height，覆盖控件显示后者。故意声明640×360但实际视频320×180的完整宿主样例通过，媒体控制相关单测扩为4项。
