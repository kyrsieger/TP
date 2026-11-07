# 参与贡献

感谢帮助改进 TP。提交改动前请先阅读根目录的[术语表](GLOSSARY.md)和[架构决策记录](docs/adr/)。术语表仅记录领域定义，ADR 记录长期决策及原因，具体行为结合代码和测试核对；需要改变规则时，请在 Pull Request 中说明原因并同步更新相关定义或决策。

## 开发与验证

工具与测试按职责分开：`scripts/` 存放开发、构建和发布工具；`tests/scripts/` 存放这些工具的 Python 测试；`tests/packaging/windows/` 存放需要真实 Windows 构建产物的 PowerShell 测试。Rust 测试保留在对应的 `crates/` 模块中。

完整本地检查还需要 Python 3.11 或更新版本，用于安装包组装测试、翻译核对和许可证审计测试。

项目使用 GPUI、Rust 和 SQLite。修改共享 Rust 代码或原生界面时，在项目根目录执行：

```sh
cargo fmt --all -- --check
rustfmt --edition 2024 --check vendor/gpui-component/src/input/state.rs vendor/gpui-component/src/input/utf16_limit.rs vendor/gpui-component/src/scroll/scrollbar.rs
python scripts/check-gpui-translations.py
python scripts/gpui_windows_identity.py
python -m unittest discover -s tests/scripts -p "test_*.py"
cargo check --workspace --locked
cargo test --workspace --locked
cargo build -p tp-gpui --locked
```

`python scripts/check.py` 执行上述完整检查；日常改动按影响范围选择必要检查即可。CI 在 Windows 和 macOS 上执行原生检查，配置不等于已通过实际平台验证。macOS 构建需要完整 Xcode 和 Metal 编译工具。

使用 `cargo run -p tp-gpui --locked` 启动，`python scripts/build_native.py` 构建正式程序。开发验证使用 Debug 构建，并设置 `TP_DIAGNOSTIC_DATA_DIR` 指向独立测试目录；未设置时读取正常应用数据目录。

隔离 Debug 默认自动显示面板。测试正常静默启动时，可在启动诊断脚本前设置 `TP_DIAGNOSTIC_START_HIDDEN=1`，再通过测试快捷键或托盘唤回；正式构建始终静默启动，不接受该变量改变显示行为。

更新原生依赖后，可运行 `python scripts/audit-gpui-licenses.py`。脚本使用本地缓存和锁文件，核对 Windows x64、macOS ARM64/x64 的普通及构建依赖，生成 `target/gpui-license-audit.json` 和包含原始许可证文本的同名 ZIP。报告列出当前依赖版本、许可证文本校验值及缺项；它是审核资料，不能替代完整发行声明，也不包含字体、图标等非 Cargo 资源的授权核对。本地缓存不全时 Cargo 会报错，需要先准备依赖。

若改动涉及系统剪贴板、文件、权限、快捷键或安装包，请说明实际验证的系统和未验证部分。三个默认忽略的 Rust 测试会操作真实系统剪贴板，不要随意启用。

## 开发版界面热更新

使用 Dioxus CLI 0.7.10 的 `dx` 启动 TP。首次启动会完整编译，随后自动打开主面板。之后保存 TP 的界面渲染代码，`dx` 会编译并发送补丁，TP 收到后会重绘窗口。可以从终端的 `TP hot reload applied` 和进程 ID 确认补丁是否应用到了当前进程。

```sh
dx serve --desktop --hot-patch --features hot-reload -p tp-gpui --locked --addr 127.0.0.1 --open false
```

例如，主面板的 `panel.rs` 中 `render_view` 里的边框、圆角和间距适合用这个流程调试。改动状态结构、初始化代码或依赖后，请用 `Ctrl+C` 结束会话再重新运行。调试时可以继续设置 `TP_DIAGNOSTIC_DATA_DIR` 使用独立数据目录。

Windows x64 请用仓库里的补丁构建 `dx`。补丁包含 [Dioxus #5967 的栈溢出修复](https://github.com/DioxusLabs/dioxus/pull/5967)，并让 `dx` 使用 TP 自己的图标和版本资源。下列命令在项目 `target` 目录中准备工具，首次需要编译。

```powershell
New-Item -ItemType Directory -Force target/hot-reload-tools | Out-Null
git clone --depth 1 --branch v0.7.10 https://github.com/DioxusLabs/dioxus.git target/hot-reload-tools/dioxus-0.7.10
$dxPatch = (Resolve-Path scripts/patches/dx-0.7.10-windows-hot-reload.patch).Path
git -C target/hot-reload-tools/dioxus-0.7.10 apply $dxPatch
cargo --config profile.dev.debug=0 rustc --locked --manifest-path target/hot-reload-tools/dioxus-0.7.10/Cargo.toml -p dioxus-cli --bin dx -- -C link-arg=/STACK:16777216
```

将启动命令里的 `dx` 换成 `./target/hot-reload-tools/dioxus-0.7.10/target/debug/dx.exe`。macOS 可使用 [Dioxus 0.7.10 的对应构建](https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.10)，目前待实机验证。

## GPUI Windows 正式二进制检查

在 Windows 上执行以下命令，随后以根 Cargo.toml 中的版本检查产物：

```powershell
./scripts/build-gpui-windows.ps1
./scripts/verify-gpui-windows.ps1 -BinaryPath ./target/release/tp-gpui.exe -ExpectedVersion 1.1.0
```

正式构建需要 Windows SDK 的 fxc.exe 编译着色器。构建脚本依次使用显式 GPUI_FXC_PATH、PATH 中的编译器或已安装 SDK 中最新的 x64 编译器，仅对当前构建设置环境变量；`-CheckToolsOnly` 可先核对工具，缓存完整时可加 `-Offline`。这避免 GPUI 默认 SDK 路径与本机已安装版本不一致导致构建失败。

检查脚本读取 PE 头、文件版本资源及 SHA-256，不启动应用；检查范围为 Windows x64、GUI 子系统、TP 产品身份和预期版本，不代表图标显示、依赖部署或安装包已验收。调试程序使用控制台子系统，会被该脚本拒绝。

release 程序不接受 `TP_DIAGNOSTIC_DATA_DIR`，运行时使用正式数据目录并恢复自动启动设置。隔离桌面验收继续使用调试入口；发布版实际运行需要另行准备隔离用户环境。

### Windows 安装包文件准备

安装器身份固定在 `packaging/windows/identity.json`，降级策略固定在 `packaging/windows/policy.json`。原生 MSI/NSIS 和 macOS 打包通过 `native_identity` 读取该清单中的产品身份，版本读取根 Cargo.toml；macOS 只使用其中的产品名和应用标识。`python scripts/gpui_windows_identity.py` 对照独立保存的 `packaging/windows/compatibility.json` 升级兼容基准检查产品名、发布者、EXE 名称、注册表路径、MSI 升级码、安装范围和语言；版本一致性由发布脚本核对根工作区与锁文件。修改这些字段前必须核对真实升级路径；版权名称不能代替原安装发布者。`python -m unittest discover -s tests/scripts -p "test_*.py" -v` 同时运行身份校验和 macOS 包结构测试。文件准备清单会带上这份兼容身份，供后续安装器使用。

`scripts/stage-gpui-windows.ps1` 为安装器准备文件目录，不安装或启动程序。显式传入正式 EXE、预期版本、完整第三方声明、Visual Studio 的 x64 `VC/Redist/MSVC/<version>/x64/Microsoft.VC143.CRT` 目录，以及 `dumpbin.exe`；后两项可省略，脚本会通过 vswhere 查找 Visual Studio C++ 工具和可再分发目录：

```powershell
./scripts/stage-gpui-windows.ps1 -BinaryPath ./target/release/tp-gpui.exe -OutputDirectory ./target/gpui-package/windows-x64 -NoticesPath /path/to/reviewed-notices.txt -RuntimeDirectory /path/to/x64/Microsoft.VC143.CRT -DumpbinPath /path/to/Hostx64/x64/dumpbin.exe -ExpectedVersion 1.1.0
```

脚本保留原安装文件名 `TP.exe`，附带项目许可证、声明及实际需要的 Visual C++ 运行库，并生成文件哈希清单。它递归核对依赖，拒绝未知非系统 DLL、缺失运行库、错误架构和未通过 Microsoft 签名验证的运行库；已有输出目录不会覆盖。运行库应来自 Visual Studio 可再分发目录，不从系统目录收集。部署方式参见 [Microsoft 本地部署说明](https://learn.microsoft.com/en-us/cpp/windows/choosing-a-deployment-method?view=msvc-170)。

该脚本只检查声明文件非空，不代替完整性审核。第三方声明由 `python scripts/audit-gpui-licenses.py` 和 `python scripts/build_gpui_notices.py` 生成，依赖缓存不全时先运行 `cargo fetch --locked`。文件准备不代表 NSIS/MSI 安装器、旧版本升级、签名或干净 Windows 环境运行已经通过验收。

Windows CI 在正式构建后运行 `./tests/packaging/windows/test-stage-gpui-windows.ps1 -BinaryPath ./target/release/tp-gpui.exe -ExpectedVersion 1.1.0`。本地也可运行同一命令；它用 vswhere 查找 C++ 工具和运行库，或接受显式 `-DumpbinPath`、`-RuntimeDirectory`。检查正常准备及文件哈希，并验证已有目录、错误版本、空声明、缺失运行库、未知 DLL 和运行库签名损坏均被拒绝。测试文件保留在唯一的 `target/gpui-stage-checks-*` 目录，均不可发布，测试不执行程序或安装器。

### Windows MSI 编译

在已完成文件准备后，使用 WiX 3.14 工具目录编译未签名 MSI：

```powershell
python scripts/package_gpui_windows_msi.py --stage target/gpui-package/windows-x64 --output target/gpui-package/msi --wix-tools /path/to/WixTools314
```

脚本重新检查准备目录的身份、版本、文件清单与哈希，拒绝额外文件和已有输出目录；然后生成 `gpui.wxs` 并调用 candle/light，不执行安装。使用中文 WiX InstallDir 界面、TP 图标、桌面/开始菜单/卸载快捷方式、安装位置注册表查找及 MSI 升级码。运行后的启动复选框仍由最终用户决定，脚本本身不会启动程序。

编译后运行 `./scripts/verify-gpui-msi.ps1 -MsiPath /path/to/TP.msi -StageDirectory /path/to/windows-x64 -DarkPath /path/to/wix/dark.exe`，以只读 Windows Installer 数据库方式核对产品属性、升级表、x64 主程序组件、载荷名称/大小、内嵌 cabinet、快捷方式、自定义动作及全部控件事件引用，并用 WiX 静态解包，逐项核对准备目录和包内文件的 SHA256。解包文件及日志保留在独立的 `target/gpui-msi-inspection-*` 目录。此命令不会执行安装动作；GUI 和真实升级仍需独立验收。

运行 `./tests/packaging/windows/test-verify-gpui-msi.ps1 -MsiPath /path/to/TP.msi -StageDirectory /path/to/windows-x64 -WixTools /path/to/wix` 可验证正常包通过、同名同大小但内容被修改的包被拒绝。该测试在独立目录复制准备文件，只修改测试副本的许可证首字节并重新编译 MSI，不运行安装，也不修改原包。

MSI 编译中的 ICE40、ICE61 提示涉及 REINSTALLMODE 和允许降级策略，应结合安装、升级及卸载行为核对。

## GPUI macOS 应用包准备

`scripts/package-gpui-macos.py` 使用 Python 3.11+，将已经在 macOS 构建的单架构 GPUI 可执行文件组装为未签名的 `.app`。名称和标识读取原生身份清单，图标读取 packaging/icons/icon.icns，版本读取根 Cargo.toml，最低系统版本读取 Mach-O 构建信息。包结构遵循 [Apple Bundle Structures](https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html)。

```sh
python scripts/package-gpui-macos.py --binary target/release/tp-gpui --output target/gpui-bundle/TP.app --notices /path/to/reviewed-gpui-notices.txt
python -m unittest discover -s tests/scripts -p test_package_gpui_macos.py -v
```

`--notices` 必须指向为 GPUI 依赖准备的完整声明；脚本只检查文件非空，不替代声明内容审核。使用生成的 `target/gpui-third-party-notices.txt`。脚本拒绝 Windows/非 macOS 文件、已有输出目录及尚未随包处理的非系统动态库。当前支持分别组装 arm64、x86_64，不接受 universal 合并二进制。

脚本不会构建、安装、签名、公证或制作 DMG。测试只验证合成输入下的包结构及错误处理；仍须在 macOS 上构建真实程序，检查 Finder 启动、托盘、图标、权限、自动启动及签名分发流程。macOS 应用包由 CI 保存为待验收产物。

## 提交 Pull Request

GPUI Component 0.5.1 使用仓库内固定源码，补丁通过根 Cargo manifest 生效，用于分组输入的 UTF-16 长度限制。更新该依赖前阅读 [补丁说明](vendor/gpui-component/TP-PATCHES.md)，保留原许可证，并复验粘贴、选区替换、撤销和输入法提交。

1. 尽量让每个 Pull Request 只解决一个明确问题，并描述复现步骤、预期结果和修改理由。
2. 使用合成剪贴板内容制作测试、截图和日志；不要提交真实数据库、私人文件、密码或令牌。
3. 功能、架构、产品规则或术语变化时同步更新对应文档。新增或更新依赖时核对 [许可资料](licenses/)。
4. 在 Pull Request 模板中列出执行过的检查与平台，区分自动测试和真实桌面验收。

提交的原创代码按照项目的 [MIT 许可证](LICENSE)分发。若贡献包含第三方代码、图片、字体或其他素材，请说明来源和许可，确保项目可以合法分发；不要直接复制没有明确授权的材料。

安全问题请按 [安全报告说明](SECURITY.md)私密提交，不要在公开 Issue 或 Pull Request 中披露。

翻译核对使用 `crates/tp-gpui/resources/translations.json`，原生专用标题、状态及消息片段另列于 `scripts/gpui-translations.json`。修改 `choose` 的字面量文案时应核对语义并更新对应词表；补充词表不得覆盖基础词表的键。检查遇到未经登记的新字面量会失败。此检查不解析完整 Rust 语法，不能代替动态消息、运行时语言切换和实际排版验证。

## 原生 CI 待验收产物

Checks 的 GPUI 作业会获取锁定依赖、生成审核 ZIP 和第三方声明。Windows 准备带运行库与文件摘要的目录；macOS 组装真实 release 二进制为 TP.app，检查 Info.plist 后用 ditto 保存为保留权限的 ZIP。每次产物名含系统、架构及提交号，标记为 gpui-unverified，保留 14 天。产物用于验收，不等于签名或正式发行，也不证明应用已实际运行。此路径不执行安装器。

原生图标、托盘图片及安装文案统一放在 `packaging/icons/` 和 `packaging/windows/installer/`；目录职责见 [原生打包资源](packaging/README.md)。标题栏标志为 `packaging/icons/tp-mark.svg`；主题配色对照数据为 `crates/tp-gpui/resources/themes.json`。

## GPUI 发布脚本模式

`scripts/release.py` 使用 Python 3.11+、Git 和 GitHub CLI。`discover` 收集已通过 main 推送 CI 的版本，`ready` 在取得标签锁后再次核对，`prepare` 核对提交、标签、main 祖先关系、原生发布标记、根 Cargo.toml 与 Cargo.lock 三个本地包的版本，并提取 CHANGELOG；`publish` 再查 CI，并只收集 `target/gpui-package/nsis/TP_<version>_x64-setup.exe` 和 `target/gpui-package/msi/TP_<version>_x64_zh-CN.msi`。

缺包、多包、版本错误、网络或认证错误均会阻止发布；两种安装包成功上传到草稿后才公开，已公开版本不会覆盖。发布测试使用模拟执行器，不上传文件。实际发布结果以对应工作流运行记录为准。

## 独立准备 NSIS 工具

运行 `python scripts/prepare_gpui_nsis.py --output target/gpui-tools/nsis`，下载锁定的 NSIS 3.11 发行包及 nsis-tauri-utils 0.5.3 插件。下载地址与 SHA-1 来自固定 Tauri CLI 源码，另检查仓库记录的 SHA-256 和 321 个发行文件摘要。脚本不执行工具，不覆盖已有目录；损坏下载在创建输出目录前拒绝。离线时可传入 `--archive` 和 `--plugin`，仍执行同样摘要检查。

编译器路径为 `target/gpui-tools/nsis/makensis.exe`，插件目录为 `target/gpui-tools/nsis/Plugins/x86-unicode/additional`。Checks Windows 作业编译 NSIS 包，再用 7-Zip 做静态载荷核对，不执行安装。WiX 和 Release 使用同样的独立工具准备流程。

## 独立准备 WiX 工具

`python scripts/prepare_gpui_wix.py --output target/gpui-tools/wix` 准备锁定的 WiX 3.14.1 发行文件，也可通过 `--archive` 提供同摘要的离线 ZIP。原始下载地址和 SHA-256 来源于固定 Tauri CLI 上游源码，文件摘要清单位于 `packaging/windows/wix/toolchain.json`。准备过程不执行工具、不覆盖已有目录；MSI 编译前会再次验证发行文件，防止缓存损坏或工具被替换。

MSI 编译传入 `--wix-tools target/gpui-tools/wix`，只读验证使用该目录下的 `dark.exe`。Checks Windows 作业包含独立工具准备、MSI 编译和静态验证，保存的 MSI 仍是未签名待验收产物。它不代表安装、升级、卸载或正式发行已经验收完成。
