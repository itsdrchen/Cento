# Cento（拾英）

Cento 是一个面向中文用户的轻量级 macOS 风格 RSS 阅读器。它用 DeepSeek API 翻译英文 RSS 标题和摘要，帮助用户快速判断一篇文章是否值得打开原文阅读。

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-FFC131?logo=tauri&logoColor=white)](https://tauri.app)
[![Platform: macOS](https://img.shields.io/badge/Platform-macOS-lightgrey.svg)]()

## 技术栈

- **桌面壳**：Tauri 2
- **前端**：HTML + CSS + 原生 JavaScript（无框架）
- **后端**：Rust + Tauri commands
- **存储**：SQLite，本地路径为 `~/Library/Application Support/io.github.itsdrchen.cento/cento.db`
- **翻译**：DeepSeek API

## 核心功能

- 添加、展示、重命名、删除 RSS 订阅源；每个订阅源可自定义 emoji 图标、刷新频率、桌面通知开关
- 后端定时调度器自动按订阅源配置的间隔抓取，关闭窗口后仍可在托盘后台运行
- 抓取时自动翻译标题与摘要，结果缓存到 SQLite；并发受信号量控制，避免突发请求
- 支持已读/未读状态，点开文章后自动标记为已读；支持星标（客户端）
- 列表顶部 segmented control 切换 全部 / 未读 / 星标
- 详情面板展示中文译标题 + 英文原标题 + 期刊 + 作者 + 发表日期 + Abstract，可切换 中文 / EN 视图
- 在系统浏览器中打开原文链接，sticky 底栏显示 DOI（从 link 自动提取）
- 阅读统计：GitHub 风格热力图、当前 / 最长连续阅读天数、订阅源阅读偏好 top 5
- **本月翻译用量**：直接读取 DeepSeek API 返回的 `usage` token 字段，按官方价目表（cache hit / miss / output 分开）换算成 CNY；同时提供 DeepSeek 官方余额查询通道
- macOS 系统通知：订阅源更新时弹出横幅；测试通知按钮验证通道是否通畅
- 浅色 / 深色主题、4 色强调色、3 档阅读字号
- 状态栏菜单（tray icon），未读数显示在 icon 右侧

## 本地运行

需要：[Node.js](https://nodejs.org) 18+、[Rust](https://www.rust-lang.org/tools/install) stable、Xcode Command Line Tools。

```bash
git clone https://github.com/itsdrchen/Cento.git
cd Cento
npm install
npm run tauri dev
```

首次启动会编译 Rust 依赖，约需 3–5 分钟。启动后在设置页填写 DeepSeek API Key，并先点击「测试连接」。

## 打包 DMG

```bash
npm run tauri build
```

产物：

```text
src-tauri/target/release/bundle/macos/Cento.app
src-tauri/target/release/bundle/dmg/Cento_<version>_aarch64.dmg
```

Release profile 已开启 `lto = true`、`codegen-units = 1`、`strip = true`、`opt-level = "z"`，单架构 DMG 通常在 3–4 MB 之间。

## 通知调试

dev 模式（`tauri dev`）下，未签名二进制无法直接通过 `UNUserNotificationCenter` 推送横幅，Cento 会自动回退到 `osascript`，但 banner 会以 "Script Editor" 名义出现。如需在 dev 模式下显示 Cento 真实 icon，可：

```bash
brew install terminal-notifier
npm run tauri build   # 注册 Cento.app 的 bundle identifier 到 LaunchServices
```

生产环境（直接运行 `Cento.app`）则自动使用原生通知插件，无需上述步骤。

## 订阅源建议

医学和生命科学期刊优先使用 PubMed 生成的 RSS，而不是出版社官网 RSS。PubMed 的 `<description>` 通常直接包含完整 Abstract，摘要覆盖率明显高于 ScienceDirect 等出版社 RSS。

在 PubMed 搜索期刊时使用期刊字段，例如 `Phytomedicine[Journal]` 或 `J Ethnopharmacol[Journal]`，然后通过页面里的 "Create RSS" 生成带 hash 的 RSS URL。PubMed 索引可能比出版社官网晚 1-3 天，但对文献分诊更稳定。

## 文档入口

- [项目结构](docs/project-structure.md)
- [架构说明](docs/architecture.md)
- [路线图与排除清单](docs/roadmap.md)
- [UI 设计规范](docs/design.md)
- [Prompt 设计](docs/prompts.md)
- [Agent 协作规则](AGENTS.md)

## 产品边界

Cento 不做完整 RSS 阅读器、文献管理器或知识库。它的核心问题只有一个：让用户更快判断"这篇内容值不值得读原文"。

## License

[MIT](LICENSE) © itsdrchen
