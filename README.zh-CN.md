# Cliporax

[English](README.md) | [简体中文](README.zh-CN.md)

**为每天大量复制、粘贴、切换上下文的人打造的隐私优先剪贴板历史工具。**

Cliporax 是一个跨平台桌面剪贴板管理器，基于 Tauri 2、React、TypeScript、Rust 和 SQLite 构建。它默认将剪贴板历史保存在本机，提供快速搜索和标签页组织能力，并通过插件、快捷键、CLI 和可选云同步支持更高级的工作流。

这个仓库同时也是 Cliporax 的 GitHub public profile 仓库，因此这份 README 兼顾项目介绍和开发者入口。

## Cliporax 关注什么

- **默认隐私优先**：剪贴板历史保存在本地，默认不包含遥测逻辑。
- **高频使用足够快**：搜索、正则搜索、虚拟滚动、置顶、多选、删除和拖拽排序都在主流程内。
- **跨平台行为可靠**：Linux、macOS、Windows 都是一等目标，包括窗口聚焦和回粘处理。
- **可扩展**：本地插件可以添加操作按钮、设置面板、预览、二维码流程和同步 UI。
- **诚实路线图**：AI OCR、语义搜索、摘要和 SQLCipher 全库加密仍是路线图项目，不会被描述成已发布能力。

## 当前功能

- 文本和图片剪贴板监听，自动写入本地 SQLite 数据库。
- 剪贴板历史列表、虚拟滚动、搜索、`regx:` 正则搜索、置顶、删除、批量选择、拖拽排序。
- 多标签页管理，支持将条目移动或复制到其他标签页。
- 敏感内容标记与清除，默认检测 `password`、`code`、`otp`、`验证码`、`secret`、`key` 等关键词。
- 全局快捷键唤出主窗口，默认 `CmdOrControl+Shift+V`，可在设置中修改。
- 复制历史项并粘贴回上一个窗口，包含 Linux/macOS/Windows 的窗口与焦点处理。
- 系统托盘、无边框窗口、置顶/取消置顶、自动隐藏、窗口尺寸和位置保存。
- 设置窗口，支持主题、列表密度、快捷键、插件和同步配置。
- 插件系统，支持插件发现、加载、启用/停用、权限授权、配置项和 UI 扩展点。
- 内置插件示例：二维码生成、二维码扫描、图片预览、Cloud Sync 设置面板。
- Cloud Sync 后端基础，支持 WebDAV、SFTP、Google Drive、OneDrive 的配置模型、凭据保存、同步状态、日志和冲突处理入口。
- 命令行工具 `cliporax-cli`，可读取、搜索、复制和保存剪贴板历史。
- 中英文界面文案。

## 技术栈

- 桌面框架：Tauri 2
- 前端：React 19、TypeScript、Vite、Tailwind CSS v4
- 状态管理：Zustand
- 后端：Rust、Tokio、SQLx
- 数据库：SQLite
- 插件：本地插件包、manifest 权限声明、前端扩展点、后端生命周期管理
- 测试：Vitest、Rust unit tests

## 项目结构

```text
.
├── src/                    # React 前端、状态、组件、插件前端运行时
├── src-tauri/              # Rust/Tauri 后端、数据库、IPC、同步、插件生命周期
├── plugins/                # 内置/示例插件包
├── scripts/                # 插件构建、CLI 准备、agent 检查脚本
├── docs/                   # 公开技术文档
├── agent/skills/           # 项目协作与检查流程说明
└── package.json            # 前端与 Tauri 开发命令
```

## 快速开始

### 环境要求

- Node.js 和 npm
- Rust stable，项目要求 Rust `1.77.2+`
- Tauri 2 所需的系统依赖
- Linux 打包/剪贴板建议安装 `xclip` 和 `x11-utils`

### 安装依赖

```bash
npm install
```

部分内置插件目录有自己的锁文件。`plugins/com.cliporax.cloud-sync` 使用 `yarn.lock`，不要无意引入 `package-lock.json`。

### 启动开发版

```bash
npm run tauri:dev
```

该命令会先准备 CLI、构建并安装内置插件，然后启动 Vite 与 Tauri 应用。

也可以只启动前端：

```bash
npm run dev
```

## 常用脚本

```bash
npm run build              # TypeScript 检查并构建前端
npm run tauri:build        # 构建桌面应用包
npm run test:run           # 运行前端测试
npm run plugins:dev        # 构建并安装内置插件
npm run codegen:types      # 从 Rust 导出 TypeScript 类型
npm run cli:build          # 构建 cliporax-cli
npm run cli -- list        # 运行 CLI 示例
```

Rust 测试：

```bash
cd src-tauri
cargo test
```

项目内快速检查：

```bash
scripts/agent/targeted-test.sh
scripts/agent/cross-platform-check.sh
scripts/agent/git-hygiene-check.sh
```

## CLI 示例

```bash
npm run cli -- list --limit 10
npm run cli -- get latest --raw
npm run cli -- search "token"
npm run cli -- copy "hello from Cliporax" --save
npm run cli -- save --file ./notes.txt
```

CLI 会尝试连接 Cliporax 创建的本地 SQLite 数据库，因此需要先运行过桌面应用并初始化数据目录。

## 插件系统

插件位于 `plugins/`，每个插件包含 `manifest.json` 和入口脚本。Manifest 描述插件 ID、名称、版本、类型、权限、扩展点和配置项。

当前内置/示例插件：

- `com.cliporax.qrcode`：为文本剪贴板项生成二维码。
- `com.cliporax.qrscanner`：从屏幕区域扫描二维码，并可写入剪贴板历史。
- `com.cliporax.imagepreview`：在独立窗口预览图片，支持缩放配置。
- `com.cliporax.cloud-sync`：为同步功能提供设置面板和状态 UI。

插件开发常用命令：

```bash
npm run plugins:build
npm run plugins:install
npm run plugins:dev
```

## 数据与隐私

- 剪贴板历史默认保存在本机应用数据目录下的 `cliporax.db`。
- 设置保存在用户配置目录下的 `cliporax/settings.json`。
- 项目默认不包含遥测逻辑。
- 后端日志应避免记录完整剪贴板内容、密钥、令牌和解密后的敏感数据。
- 同步凭据由后端保存；同步模块包含加密与解锁模型，但 SQLite 主数据库当前不是 SQLCipher 全库加密。

## 云同步状态

当前代码包含 Cloud Sync 的配置 UI、Provider 抽象、WebDAV/SFTP/Google Drive/OneDrive provider、同步 profile、后端凭据引用、加密/解锁模型、调度状态、运行报告、日志、冲突处理和插件配置同步入口。

它已经不是单纯的设置壳，但仍建议视为“可用基础已实现，生产级体验继续打磨”的能力。

同步相关代码主要在：

- `src-tauri/src/sync/`
- `src/components/Settings/CloudSyncTab.tsx`
- `plugins/com.cliporax.cloud-sync/`

## 路线图说明

- 插件系统：已实现插件发现、加载、启用/停用、卸载运行态、权限授权、配置项、前端扩展点和内置插件构建/安装脚本。仍缺少真正的在线插件市场、远程下载安装、版本更新和删除插件包的完整产品流程。
- AI 能力：当前没有实现通用图片 OCR、本地语义搜索或文本摘要。二维码扫描插件能识别二维码，但不等同于 OCR/AI 检索能力。
- 本地加密：同步模块已有 Argon2id + authenticated encryption 的远端同步加密模型，provider 凭据也通过后端保存；主 SQLite 剪贴板数据库目前不是 SQLCipher 全库加密。
- 云同步：已实现 Cloud Sync 设置 UI、同步 profile、WebDAV/SFTP/Google Drive/OneDrive provider、凭据引用、连接测试、调度、日志、冲突入口和可选加密模型。下一步重点是更多真实服务集成验证、冲突 UX、凭据存储硬化和跨平台运行测试。
- 打包发布：已有 Tauri 构建脚本和 Linux `deb`/`rpm` bundle 配置。macOS/Windows 打包配置和发布流水线还需要补齐与验证。

## 文档

- [CLI 使用指南](docs/cli-usage.md)
- [插件系统设计](docs/plugin-system-design.md)
- [Cloud Sync 架构](docs/cloud-sync-architecture.md)

## 许可证

Cliporax 使用 [MIT License](LICENSE) 授权。
