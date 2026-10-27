# 参与贡献

感谢帮助改进 TP。提交改动前请先阅读根目录的[术语表](GLOSSARY.md)和[架构决策记录](docs/adr/)。术语表仅记录领域定义，ADR 记录长期决策及原因，具体行为结合代码和测试核对；需要改变规则时，请在 Pull Request 中说明原因并同步更新相关定义或决策。

## 开发与验证

本项目使用 Node.js 24、Rust stable、Tauri 2。桌面构建还需要目标系统的 Tauri 前置工具。当前开发环境只在 Windows 11 上实际运行过；macOS 代码不能仅凭 Windows 测试判定通过。

```sh
npm ci
npm test
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
```

界面回归可运行 `npx playwright install chromium` 和 `npm run test:ui`。若改动涉及系统剪贴板、文件、权限、快捷键或安装包，请额外说明在哪个系统上做了真实桌面验证，以及未验证的部分。三个默认忽略的 Rust 剪贴板测试会操作真实系统剪贴板，不要在未备份剪贴板内容时直接运行。

## 提交 Pull Request

1. 尽量让每个 Pull Request 只解决一个明确问题，并描述复现步骤、预期结果和修改理由。
2. 使用合成剪贴板内容制作测试、截图和日志；不要提交真实数据库、私人文件、密码或令牌。
3. 功能、架构、产品规则或术语变化时同步更新对应文档。新增或更新依赖时核对 [第三方声明](public/third-party-notices.txt)。
4. 在 Pull Request 模板中列出执行过的检查与平台，区分自动测试和真实桌面验收。

提交的原创代码按照项目的 [MIT 许可证](LICENSE)分发。若贡献包含第三方代码、图片、字体或其他素材，请说明来源和许可，确保项目可以合法分发；不要直接复制没有明确授权的材料。

安全问题请按 [安全报告说明](SECURITY.md)私密提交，不要在公开 Issue 或 Pull Request 中披露。
