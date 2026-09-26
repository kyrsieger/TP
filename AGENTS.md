# AGENTS.md

## 项目简介

本项目是面向 Windows 11 和 macOS 的本地剪贴板历史工具。它保存文字、链接、图片和文件，支持搜索、类型筛选、收藏、自建分组、去重、自动清理，以及通过小面板和快捷键应用历史记录。视频作为文件记录，使用首帧预览。产品规则以 [产品设计](docs/DESIGN.md) 为准。

当前已开始使用 Tauri 2、TypeScript、Rust 和 SQLite 实现。macOS 和 Windows 11 的代码都必须实现；当前机器只有 Windows，现阶段只在 Windows 11 上运行验证。不要把未在 Mac 上运行的功能写成已通过 macOS 验证。

## 文档入口

- [README](README.md)：项目概览与文档索引。
- [产品设计](docs/DESIGN.md)：记录、文件、清理、分组、面板和操作规则。
- [技术结构](docs/TECHNICAL-ARCHITECTURE.md)：架构、平台适配与验证顺序。系统能力仍需实测。
- [术语表](docs/GLOSSARY.md)：统一术语；发现歧义时先查阅此文档。
