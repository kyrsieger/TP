# TP

面向 Windows 11 和 macOS 的本地剪贴板历史工具。当前原生实现使用 GPUI、Rust 和 SQLite。

开发与构建使用 Rust 和 Python。Windows 发布流程见 [版本发布说明](docs/releasing.md)。
- 保存文字、链接、图片和文件，视频文件支持首帧预览。
- 支持搜索、类型筛选、去重、收藏、自建分组和自动清理。
- 通过系统托盘或菜单栏打开面板，也可以自行设置全局快捷键。
- 数据保存在本机，不提供账户、云端服务或跨设备同步。

目前在 Windows 11 上开发和验证。macOS 适配代码已提供，编译、权限和实际运行仍待 Mac 环境验证；暂不承诺 macOS 安装包可用性。

## 使用与隐私

启动后 TP 静默常驻，只显示托盘或菜单栏图标。单击图标打开面板，在设置中录入唤出快捷键；全局快捷键没有默认值。应用历史记录时会将内容写回剪贴板，并尝试返回先前窗口发送粘贴键，实际粘贴效果取决于目标应用和系统权限。

**剪贴板内容可能包含密码、令牌和私人文件。TP 在本机保存数据，但不加密，不能作为密码保险箱使用。**

- 数据目录包含 SQLite 数据库、图片和部分文件副本。默认位置为 Windows 的 `%APPDATA%\TP\`、macOS 的 `~/Library/Application Support/TP/`；实际位置以系统返回的应用数据目录为准。
- 普通记录默认保留 300 条、7 天，保留时间从最后使用起算。收藏或加入自建分组的记录不受自动清理约束，需要手动删除。
- 从 1Password **桌面应用**识别出的普通记录默认 90 秒后删除；这不代表能识别所有密码或所有密码管理器。收藏或移入自建分组后，也不再自动删除。
- 到期清理删除 TP 中的记录及其附件，不会清空系统剪贴板，也没有实现安全擦除。
- 分享截图、日志或问题复现材料前，请使用合成数据并移除个人内容。

视频首帧预览依赖系统 `PATH` 中的 `ffmpeg`。它是可选依赖，项目不附带 FFmpeg；缺少时仍可记录、应用视频文件，但不生成首帧预览。

## 开发环境

开发需要 Git、Rust stable 和 Python 3.11+（脚本仅使用标准库），无需 Node.js/npm。Rust 依赖以根目录 `Cargo.lock` 为准。
### Windows 11

安装以下工具：

1. Git 和 Rust，Rust 选择 MSVC 工具链。
2. Visual Studio 2022 Build Tools，勾选“使用 C++ 的桌面开发”，包含 MSVC 与 Windows SDK。
3. Windows SDK 中的 `fxc.exe`，供 GPUI 编译着色器。正式构建脚本会自动寻找已安装 SDK，也支持通过 `GPUI_FXC_PATH` 指定路径。

首次构建需要联网下载 Rust 依赖；安装包工具及验证方法见 [贡献说明](CONTRIBUTING.md)。
### macOS（待验证）

在 Mac 上安装 Git、Rust 和完整 Xcode，并完成 Xcode 初始化。GPUI 需要 Metal 编译工具，仅安装 Command Line Tools 不足以保证可构建。检查：

```sh
xcodebuild -version
xcrun --sdk macosx --find metal
xcrun --sdk macosx --find metallib
```

在“系统设置 → 隐私与安全性”中，跨应用模拟粘贴可能需要为 TP 开启“辅助功能”；通过 System Events 切换前台应用时，也可能出现“自动化”授权提示。这些权限和实际行为尚未实机验证。

请在对应系统上构建安装包。macOS 面向他人分发所需的签名和公证尚未完成配置。

## 本地运行

在项目根目录构建 GPUI 原生程序：

使用 Cargo 运行开发版，使用 `python scripts/build_native.py` 构建当前系统的正式程序。Windows 构建脚本会自动查找 SDK 着色器编译器。
```sh
cargo build -p tp-gpui --locked
cargo test --workspace --locked
```

使用 `cargo run -p tp-gpui --locked` 可启动原生程序。未设置诊断目录时，它使用正常 TP 数据目录，并操作真实系统剪贴板。开发验证应使用独立数据目录和合成记录；Windows 已准备好测试数据库时，可运行：

```powershell
./scripts/run-gpui-diagnostic.ps1 -DataDirectory ./target/my-fixture
```

测试目录必须已包含 `history.sqlite3`，该脚本会先验证目录再启动 Debug 程序。诊断目录设置仅用于 Debug 构建，不适用于正式二进制。详细检查命令见 [贡献说明](CONTRIBUTING.md)。

调试界面时，按[热更新工具准备说明](CONTRIBUTING.md#开发版界面热更新)配置一次 `dx`，之后运行 `python scripts/dev.py`。保存 Rust 界面代码后，可以在运行中的 TP 窗口查看效果。

## GPUI 正式构建与安装包

在 Windows 上，从项目根目录运行一条命令即可构建并校验 x64 的 EXE 和 MSI 安装包：

```powershell
python scripts/package.py
```

脚本从根目录 `Cargo.toml` 读取 TP 版本，不需要输入版本号。它会下载打包工具，并将两个未签名的安装包放到 `target/windows-installers/` 下本次打包专用的子目录，完成后打印文件路径。除了上述 Windows 构建环境，还需要安装 PowerShell 7 和 7-Zip，并确保 `pwsh.exe` 和 `7z.exe` 在 `PATH` 中。

只构建 Windows 正式二进制：

```powershell
./scripts/build-gpui-windows.ps1
./scripts/verify-gpui-windows.ps1 -BinaryPath ./target/release/tp-gpui.exe -ExpectedVersion 1.1.0
```

macOS 正式二进制（须在 Mac 上执行，待实机验证）：

```sh
cargo build -p tp-gpui --release --locked
```

Windows NSIS/MSI 和 macOS 应用包的组装脚本已提供。发布流程仅支持带 `packaging/release.json` 原生标记的版本，上传前核对两种 Windows 安装包；不再构建历史 Tauri 标签。分发材料最终审核、安装升级、macOS 实机与远程发布验证仍独立进行。macOS 应用包目前作为 CI 待验收产物。

应用和安装包名称为 TP，Rust 工作区包含 `tp-core`、`tp-desktop`、`tp-gpui`，应用标识保持为 `TP`。旧数据迁移规则见[应用标识和数据目录决策](docs/adr/0009-app-identity-and-data-directory.md)。

## 开发方式与贡献

本项目完全由 AI 开发，开发过程中使用了 [mattpocock/skills](https://github.com/mattpocock/skills) 技能库中的部分技能。

文档管理主要使用其中的 [domain-modeling](https://github.com/mattpocock/skills/tree/main/skills/engineering/domain-modeling) 技能，维护[领域术语表](GLOSSARY.md)和[架构决策记录（ADR）](docs/adr/)。参与贡献时，你也可以选择使用该技能，配合 AI 维护这些文档；具体贡献说明见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可证

项目原创代码采用 [MIT 许可证](LICENSE)，Copyright (c) 2026 kyrsieger。第三方依赖和参考资料保留各自许可，原生声明由 `python scripts/audit-gpui-licenses.py` 和 `python scripts/build_gpui_notices.py` 生成，原始许可资料位于 [licenses](licenses/)。更新依赖时需同步核对声明，分发安装包时保留许可证和第三方声明。
