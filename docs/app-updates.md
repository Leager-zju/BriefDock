# 应用自动更新

默认在启动时和每15分钟后台检查、下载并准备应用更新；这是全局能力，切换新闻/红果、进入设置或隐藏窗口不会停止它。隔离实例默认不自动联网。系统设置可关闭自动检查，也可随时手动检查。准备好后底部显示「重启更新」，点击后正常保存、退出并替换程序；不会自动打断播放或强行重启。

## 测试包与信任

测试包复用原下载入口，不能只比较2.0.0版本号。基础包携带build-info.json，记录本包文件哈希；规范化文件表的SHA256作为构建标识，同版本内容变化仍会被识别为更新。app-update-source.json固定独立更新目录URL和维护者Ed25519公钥，使用已有发布身份，不从下载响应信任新公钥。

固定目录为https://lazybuddydev.github.io/LazyBuddy/updates/windows-x86_64.json及对应.sig。先验证原文签名、平台、版本/构建、ZIP大小及哈希，再验证build-info与每个文件哈希。ZIP拒绝越界、链接、重复及非应用文件；更新仅覆盖宿主/CLI/辅助程序、可信源配置与许可说明，不替换模块、账号、用户设置或其他目录。元数据1MiB、ZIP256MiB、解压512MiB有界；沿用Windows TLS、系统代理与慢网下载保护。

## Windows替换

已校验的独立lazybuddy-updater.exe复制到更新缓存运行，防止更新自身时被占用。辅助进程核对父进程真实路径、获取独占安装锁，准备就绪后才让宿主正常退出，等待最多30秒。替换前备份已有文件，失败恢复已替换项；新宿主的PID和窗口控制接口就绪后才确认成功。准备或等待失败不覆盖仍在运行的应用，无法完整恢复时保留备份供修复，不能声称已恢复。

下载缓存位于当前数据目录updates；成功后清理解压、ZIP和备份，仍运行的辅助EXE下次检查再回收，不停止用户实例清理占用。只在受控隔离实例验证替换，不使用真实账号/播放做更新验收。Windows进程等待依据[Microsoft文档](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)，独立进程处理Windows运行文件占用的模式亦参考[self-replace](https://docs.rs/self-replace/latest/self_replace/)。

## 发布步骤

```powershell
cargo build --release --locked -p lazybuddy -p lazybuddy-cli -p lazybuddy-app-updater
python -B scripts/package_base.py --output target/release-base
.\target\release-tools\Scripts\python.exe -B scripts/sign-update.py --manifest target/release-base/app-update.json --protected-key "$env:LOCALAPPDATA\LazyBuddyDev\signing\module-catalog-ed25519.dpapi" --output docs/updates/windows-x86_64.json --replace
```

package_base生成基础ZIP、package.json和未签名app-update.json；默认URL仍是当前测试包，稳定发版时显式传--download-url。签署目录后先覆盖上传ZIP/校验值，再推送docs/updates原文与签名到Pages；.gitattributes禁止对已签名JSON换行转换。短暂目录/包不同步时校验失败并保留现有应用，可稍后检查，绝不关闭校验或回退到未签名更新。

仅在首次启用自动更新的旧客户端需要引导安装；启用后未来测试包覆盖会自动获取，用户只需点击重启更新。模块仍由功能管理按独立签名目录更新。

## 定向验证

运行cargo test -p lazybuddy-app-updater --lib、cargo test -p lazybuddy updater::tests、python -B scripts/verify/test-app-release.py。共享下载代码变化才额外检查module-manager的package单元；不默认跑工作区、真实业务或全量界面。实际进程替换由独立更新链路脚本验证，不能把文件复制单元成功当作线上更新成功。
