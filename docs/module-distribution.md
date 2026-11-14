# 模块签名与在线分发

LazyBuddy 预览版的主程序和功能分开分发。根目录 `module-catalog.json` 是信任根配置，包含固定 Ed25519 公钥及下载站 HTTPS 目录地址；Cargo 构建自动复制到主程序旁，`scripts/package_base.py` 默认携带该文件。普通用户在「功能」中安装、更新或卸载，依赖自动处理，无需退出或重启。

## 当前发布

- 主程序：[v2.0.0-preview.1](https://github.com/LazyBuddyDev/LazyBuddy/releases/tag/v2.0.0-preview.1)，基础 ZIP 不含功能及重运行时。
- 模块：[modules-v0.1.0](https://github.com/LazyBuddyDev/LazyBuddy/releases/tag/modules-v0.1.0)，含 news、hongguo、python-runtime、media-runtime 四包、catalog.json 和原始 64 字节 catalog.json.sig。
- 模块由各服务仓库独立构建，再集中托管发布；服务源码不进入主仓库。news 来源 7e53803，hongguo 和运行时来源 68cac34。

下载目录后先验签，再使用其哈希/长度校验 ZIP，并检查清单、路径、平台及精确依赖。下载响应中的公钥不能替换随客户端分发的公钥。失败不回退到未签名安装。签名不等于 Windows Authenticode，此预览 EXE 未申请代码签名证书。

## 签名身份

本机维护者私钥保存在 `%LOCALAPPDATA%/LazyBuddyDev/signing/module-catalog-ed25519.dpapi`，不在工作区内。采用 Windows 当前用户 DPAPI 保护；仅解密到签名进程内存，不输出私钥，不以明文落盘或存入 GitHub。依据 [Microsoft DPAPI 文档](https://learn.microsoft.com/en-us/windows/win32/seccrypto/example-c-program-using-cryptprotectdata)，通常需要同一机器、同一 Windows 用户才能解密。

安装维护工具：`python -m venv target/release-tools`，然后使用该环境 `python -m pip install cryptography==46.0.3`。已存在的私钥必须复用，不在普通构建中生成。首次受权建立新身份时，显式使用 `--initialize-key`，路径存在则拒绝覆盖。DPAPI 文件本身不是跨机器恢复备份；更换机器前须规划安全迁移，丢失身份则需更新随客户端分发的公钥，不可静默换钥。

```powershell
# 先从两个服务仓库获取已验证的四个不可变 ZIP；输出文件必须尚不存在。
python -B scripts/assemble-catalog.py --packages <news.zip> <hongguo.zip> <python-runtime.zip> <media-runtime.zip> --base-url <HTTPS模块资源目录> --output target/release-catalog/catalog.json

# 复用既有身份；不会重新生成私钥。配置输出先放临时目标，核对公钥再更新正式配置。把目录及签名原样复制到 docs/modules，发布到固定下载站地址。
.\target\release-tools\Scripts\python.exe -B scripts/sign-catalog.py --catalog target/release-catalog/catalog.json --protected-key "$env:LOCALAPPDATA\LazyBuddyDev\signing\module-catalog-ed25519.dpapi" --source target/release-catalog/module-catalog.json --url <HTTPS目录地址>

cargo build --release --locked -p lazybuddy -p lazybuddy-cli
python -B scripts/package_base.py --output target/release-base
```

## 更新与验收

- 包、目录和配置一起核对。正式版前测试安装包覆盖既有预览入口和资源，不新增预览流水发布；同步更新来源提交、SHA256与验证证据。业务或适配实现变化仍增加模块内部版本及精确依赖，再覆盖测试目录，确保已安装客户端能够识别更新；正式版保持不可变版本快照。
- 客户端固定下载站 `https://lazybuddydev.github.io/LazyBuddy/modules/catalog.json`，`docs/modules` 同时维护目录原文与签名；Release 保留各次不可变快照及模块 ZIP。上线新模块时更新下载站的签名目录，现有客户端在「功能」点「刷新」后可安装/更新，无需重发宿主。目录 JSON 与 .sig 必须按原始字节提交（.gitattributes 禁止换行转换）；目录与签名短暂不同步时验签失败，稍后重试，不能关闭校验。
- 先创建草稿 Release 并上传完整资源，核对后发布；主程序包必须包含与已发布目录匹配的公钥配置。
- 公布后通过匿名 HTTPS 重新下载目录、签名及模块，验证真实客户端安装、依赖、卸载与运行中入口。真实账号/节目权限、收藏写入和进度上传是独立验收，不能由分发成功外推。
- 在线验证使用独立数据目录，不读取真实凭据，不触发真实账号或新闻上游请求；完成后退出自身实例并清理一次性下载、模块副本和截图。

HTTPS使用显式选择NativeTls的ureq Agent，目录总超时30秒、每包300秒、连接15秒；保留证书校验和签名校验。ureq启用native-tls特性仍不会自动选择该后端，见 [ureq官方文档](https://docs.rs/crate/ureq/3.5.1)。本机无效TLS对端单元测试及真实在线安装分别验证配置与端到端链路。

Windows启用ureq的win-system-proxy支持：显式代理环境变量优先，没有时使用当前用户已启用的手动系统代理。不会修改系统设置；不支持由此推断PAC/WPAD已经适配。当前覆盖测试构建修复从资源管理器启动时未读取系统代理的问题，模块包及签名目录不变。回归使用 `test-online-modules.py --without-proxy-env` 验证完整下载事务。
