# TP

面向 Windows 11 和 macOS 的本地剪贴板历史工具，使用 Tauri 2、React、TypeScript、Rust 和 SQLite 构建。

- 保存文字、链接、图片和文件，视频文件支持首帧预览。
- 支持搜索、类型筛选、去重、收藏、自建分组和自动清理。
- 通过系统托盘或菜单栏打开面板，也可以自行设置全局快捷键。
- 数据保存在本机，不提供账户、云端服务或跨设备同步。

目前在 Windows 11 上开发和验证。macOS 适配代码已提供，编译、权限和实际运行仍待 Mac 环境验证；暂不承诺 macOS 安装包可用性。

## 使用与隐私

启动后 TP 静默常驻，只显示托盘或菜单栏图标。单击图标打开面板，在设置中录入唤出快捷键；全局快捷键没有默认值。应用历史记录时会将内容写回剪贴板，并尝试返回先前窗口发送粘贴键，实际粘贴效果取决于目标应用和系统权限。

**剪贴板内容可能包含密码、令牌和私人文件。TP 在本机保存数据，但不加密，不能作为密码保险箱使用。**

- 数据目录包含 SQLite 数据库、图片和部分文件副本。默认位置为 Windows 的 `%APPDATA%\com.tp.clipboard\`、macOS 的 `~/Library/Application Support/com.tp.clipboard/`；实际位置以系统返回的应用数据目录为准。
- 普通记录默认保留 300 条、7 天，保留时间从最后使用起算。收藏或加入自建分组的记录不受自动清理约束，需要手动删除。
- 从 1Password **桌面应用**识别出的普通记录默认 90 秒后删除；这不代表能识别所有密码或所有密码管理器。收藏或移入自建分组后，也不再自动删除。
- 到期清理删除 TP 中的记录及其附件，不会清空系统剪贴板，也没有实现安全擦除。
- 分享截图、日志或问题复现材料前，请使用合成数据并移除个人内容。

视频首帧预览依赖系统 `PATH` 中的 `ffmpeg`。它是可选依赖，项目不附带 FFmpeg；缺少时仍可记录、应用视频文件，但不生成首帧预览。

## 开发环境

推荐 Node.js 24 LTS（含 npm）和通过 [rustup](https://rustup.rs/) 安装的 Rust stable。本项目本地验证使用 Node.js 24.16.0、Rust 1.98.1；这组版本是验证基线，并非经过逐版本测试的最低要求。依赖版本以 `package-lock.json` 和 `src-tauri/Cargo.lock` 为准。

### Windows 11

安装以下工具，详细步骤参见 [Tauri 2 环境准备](https://v2.tauri.app/start/prerequisites/)：

1. Git、Node.js 和 Rust，Rust 选择 MSVC 工具链。
2. Visual Studio 2022 Build Tools，勾选“使用 C++ 的桌面开发”，包含 MSVC 与 Windows SDK。
3. Microsoft Edge WebView2 Runtime，Windows 11 通常已经安装；缺失时安装 Evergreen Runtime。

默认构建包含 NSIS 和 MSI 两种安装包；MSI 使用的 WiX 工具还需要系统 VBScript 功能。只构建 NSIS 可使用下方指定命令。第一次构建可能需要联网下载 Rust 依赖、NSIS 或 WiX 工具。

### macOS（待验证）

在 Mac 上安装 Git、Node.js、Rust 和 Xcode Command Line Tools：

```sh
xcode-select --install
```

也可以安装并启动完整 Xcode 完成初始化。在“系统设置 → 隐私与安全性”中，跨应用模拟粘贴可能需要为 TP 开启“辅助功能”；通过 System Events 切换前台应用时，也可能出现“自动化”授权提示。这些权限和实际行为尚未实机验证。

请在对应系统上构建安装包。macOS 面向他人分发时的 Developer ID 签名和公证需另行配置，参见 [Tauri macOS 签名说明](https://v2.tauri.app/distribute/sign/macos/)。

## 本地运行

```sh
git clone https://github.com/kyrsieger/TP.git
cd TP
npm ci
npm run tauri dev
```

仓库保持私有期间，克隆需要有访问权限的 GitHub 账户。开发服务使用 `http://localhost:5173`。只运行 `npm run dev` 会启动前端服务，完整剪贴板功能需要 Tauri 桌面宿主。

## 构建安装包

在项目根目录执行：

```sh
npm ci
npm run tauri -- build -- --locked
```

也可以只构建指定格式：

```sh
# Windows：NSIS 安装程序
npm run tauri -- build --bundles nsis -- --locked

# Windows：MSI 安装程序
npm run tauri -- build --bundles msi -- --locked

# macOS：应用与 DMG（须在 Mac 上执行，待验证）
npm run tauri -- build --bundles app,dmg -- --locked
```

默认输出位于 `src-tauri/target/release/bundle/` 下的 `nsis/`、`msi/`、`macos/` 或 `dmg/` 子目录。显式指定 Rust target 或 `CARGO_TARGET_DIR` 时，输出路径会相应变化。`npm run build` 只构建前端，不生成桌面程序或安装包。

应用、主程序和安装包名称为 TP，npm 与 Rust 包名为 `tp`，应用标识为 `com.tp.clipboard`。升级时保持应用标识，以沿用已有安装身份和本地数据目录。

## 许可证

项目原创代码采用 [MIT 许可证](LICENSE)，Copyright (c) 2026 kyrsieger。第三方依赖和参考资料保留各自许可，详见[第三方声明](public/third-party-notices.txt)。更新依赖时需同步核对声明，分发安装包时保留许可证和第三方声明。
