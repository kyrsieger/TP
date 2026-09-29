# TP

这是一个面向 Windows 11 和 macOS 的本地剪贴板历史工具。它记录文字、链接、图片和文件，提供去重、搜索、收藏、自建分组及快捷键操作；视频以视频文件形式记录并显示首帧预览。

项目已开始实现，采用 Tauri 2、TypeScript 和 Rust。记录、分组与设置保存在本机 SQLite；图片及小文件副本保存在应用数据目录。当前开发机器只有 Windows，macOS 的编译与运行验证留待获得 Mac 环境后进行。

应用、主程序和安装包的名称为 TP；npm 与 Rust 包名使用小写 `tp`。Tauri 应用标识保留为 `com.tp.clipboard`，以沿用现有安装身份与本地数据目录。

## 项目文档

- [产品设计](docs/DESIGN.md)：功能与交互规则。
- [系统架构](docs/SYSTEM-ARCHITECTURE.md)：系统边界、模块协作、数据与关键流程、平台差异及当前限制。
- [术语表](docs/GLOSSARY.md)：统一产品与实现术语。

## 本地运行

需要 Node.js、Rust 和 Tauri 2 对应的平台构建工具。在项目根目录运行：

```sh
npm install
npm run tauri dev
```

首次启动会显示面板；关闭窗口后可从系统托盘再次打开。全局唤出快捷键没有默认值，需在面板设置中录入。`npm run build` 构建前端，`cd src-tauri && cargo check` 检查 Rust 后端。

视频首帧预览目前使用系统 `PATH` 中的 `ffmpeg`；缺少它时，视频文件仍可记录和应用，但不会显示预览。跨应用自动粘贴会尝试返回先前窗口并发送粘贴键，是否进入目标输入框取决于目标应用和系统权限。
