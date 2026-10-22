# AGENTS.md

## 工具简介

本工具是面向 Windows 11 和 macOS 的本地剪贴板历史工具。它保存文字、链接、图片和文件，支持搜索、类型筛选、收藏、自建分组、去重、自动清理，以及通过小面板和快捷键应用历史记录。视频作为文件记录，使用首帧预览。产品规则以 [产品设计](docs/DESIGN.md) 为准。

当前使用 Tauri 2、React、TypeScript、Rust 和 SQLite 实现。macOS 和 Windows 11 的代码都必须实现；当前机器只有 Windows，现阶段只在 Windows 11 上运行验证。不要把未在 Mac 上运行的功能写成已通过 macOS 验证。

## 文档入口

- [产品设计](docs/DESIGN.md)：记录、文件、清理、分组、面板和操作规则。
- [系统架构](docs/SYSTEM-ARCHITECTURE.md)：系统边界、模块协作、数据与关键流程、平台适配及验证状态。
- [术语表](docs/GLOSSARY.md)：统一术语；发现歧义时先查阅此文档。
- [应用标志](design/logo/final/README.md)：`design/logo/` 保存正式 logo 的标准与小尺寸 SVG 源文件、配色说明和图标生成脚本。修改标志时从源文件重新生成；应用实际使用的资源位于 `src-tauri/icons/` 和 `public/`，候选稿、预览图和临时导出不在该目录保留。

## 文档同步

修改功能、架构、产品设计或术语等内容时，同步更新对应文档，确保文档与实现一致。
