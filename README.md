# TP

这是一个面向 Windows 11 和 macOS 的本地剪贴板历史工具。它记录文字、链接、图片和文件，提供去重、搜索、收藏、自建分组及快捷键操作；视频以视频文件形式记录并显示首帧预览。

项目采用 Tauri 2、React、TypeScript 和 Rust。主面板与两个预览页面都由 React 渲染。记录、分组与设置保存在本机 SQLite；图片及小文件副本保存在应用数据目录。当前开发机器只有 Windows，macOS 的编译与运行验证留待获得 Mac 环境后进行。

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

开发服务使用 `http://localhost:5173`，Vite 与 Tauri 的地址配置保持一致。

启动后应用静默常驻，只在系统托盘或菜单栏显示图标，不主动显示面板；左键单击托盘图标或在面板设置中录入唤出快捷键即可打开。全局唤出快捷键没有默认值，需在面板设置中录入。`npm run build` 构建前端，`cd src-tauri && cargo check` 检查 Rust 后端。

视频首帧预览目前使用系统 `PATH` 中的 `ffmpeg`；缺少它时，视频文件仍可记录和应用，但不会显示预览。跨应用自动粘贴会尝试返回先前窗口并发送粘贴键，是否进入目标输入框取决于目标应用和系统权限。

## 模块与验证

前端入口只挂载 React：`src/App.tsx` 组合主面板组件，`src/components` 和 `src/preview` 负责展示，`src/state` 负责会话、选择、设置提交和分组编辑，`src/services` 负责桌面接口、窗口几何、预览协调及焦点，`src/domain` 保存数据契约和纯计算。`src/application.ts` 组装依赖，`src/input` 处理键盘规则。

Rust 的 `src-tauri/src/app.rs` 负责宿主组装与命令注册，业务实现位于 `src-tauri/src/app/`：数据模型、存储、清理、记录操作、文件、媒体、剪贴板、窗口与后台任务各自成模块。详细职责及依赖方向见[系统架构](docs/SYSTEM-ARCHITECTURE.md)。

```sh
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib
npx playwright install chromium
npm run test:ui
```

浏览器测试会启动端口 5175 的本地 Vite 服务，使用 `tests/fixtures` 中的模拟 IPC 和合成记录，不访问真实剪贴板、数据库或系统设置。可通过 `TP_TEST_BROWSER` 环境变量指定已有 Chromium 的完整可执行路径。浏览器回归不能替代桌面剪贴板、跨应用粘贴和系统快捷键的实机验收。
