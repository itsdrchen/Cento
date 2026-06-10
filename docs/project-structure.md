# 项目结构

本文说明 Cento 当前文件夹和主要文件的职责。README 只保留项目入口信息；代码和文档的细节以本文为准。

## 顶层目录

```text
Cento/
├── AGENTS.md                 # AI agent 执行任务时的基本原则
├── README.md                 # 项目简介、启动方式、文档入口
├── docs/                     # 长期维护的设计、架构、路线图文档
├── scripts/                  # 辅助脚本，例如图标构建和本地启动脚本
├── src/                      # 前端代码
├── src-tauri/                # Tauri/Rust 后端与桌面壳配置
├── package.json              # 前端/Tauri CLI 脚本
├── package-lock.json         # npm 依赖锁定
└── node_modules/             # 本地 npm 依赖，不手动编辑
```

## 前端：`src/`

```text
src/
├── index.html                # 应用 DOM 结构：toolbar / 三栏主界面 / 设置 view (rail + content)
├── main.js                   # 前端状态机、事件绑定、Tauri invoke 调用、渲染、客户端持久化、阅读统计
└── styles.css                # 全局样式、token 层（浅/深 × 4 强调色 × 3 字号档）、组件层
```

前端保持零框架。`main.js` 目前是单文件实现，后续只有在明显降低复杂度时再拆分。

### 客户端 localStorage 键

不进 SQLite 的轻量偏好和缓存，全部走 localStorage：

| key | 含义 |
|---|---|
| `theme` | `light` / `dark` |
| `accent` | `coral` / `blue` / `forest` / `ink` |
| `font-scale` | `sm` / `md` / `lg` |
| `sidebar-collapsed` | `"0"` / `"1"` |
| `feed-emoji-{feedId}` | 该订阅源的 emoji 图标 |
| `starred-ids` | 已星标的 entry id 数组（JSON） |
| `cost-{YYYY-MM}` | 该月累计翻译字符数 |

`<body>` 上的 `data-theme` / `data-accent` / `data-font-scale` 三个属性驱动整个主题系统（见 `styles.css` 的 token 层）。

## 后端：`src-tauri/`

```text
src-tauri/
├── Cargo.toml                # Rust crate 依赖：tauri、rusqlite、reqwest、feed-rs 等
├── Cargo.lock                # Rust 依赖锁定
├── tauri.conf.json           # Tauri 窗口、identifier、bundle 配置
├── build.rs                  # Tauri 构建脚本
├── capabilities/             # Tauri 2 capability 权限配置
├── icons/                    # App 图标资源，cento.svg 为源文件，cento.icns 为 macOS bundle 图标
└── src/                      # Rust 应用代码
```

## Rust 代码：`src-tauri/src/`

```text
src-tauri/src/
├── main.rs                   # 二进制入口，只调用 cento_lib::run()
├── lib.rs                    # Tauri app 初始化、数据库状态注入、command 注册
├── db.rs                     # SQLite 建表、轻量迁移、连接初始化
├── models.rs                 # 前后端共享的序列化数据结构
├── commands/                 # Tauri command 层，负责 JS 可调用接口
└── services/                 # 业务逻辑层，负责数据库、网络请求和数据处理
```

## Commands 层

```text
src-tauri/src/commands/
├── mod.rs                    # commands 模块声明
├── feed_cmd.rs               # add_feed / list_feeds / delete_feed / rename_feed
├── fetch_cmd.rs              # fetch_all_feeds
├── entry_cmd.rs              # list_entries / fetch_abstract / set_entry_read
├── settings_cmd.rs           # get_settings / save_settings / test_connection
└── translate_cmd.rs          # translate_title / translate_summary / open_url
```

Command 层尽量薄：读取参数、取 Tauri state、调用 service。异步 command 里读完 DB 后要释放锁，再进行 `.await`。

## Services 层

```text
src-tauri/src/services/
├── mod.rs                    # services 模块声明
├── feed_service.rs           # 订阅源 CRUD
├── fetch_service.rs          # RSS 拉取、feed-rs 解析、去重入库、标题自动翻译
├── entry_service.rs          # 文章列表查询，合并翻译缓存和元数据
├── article_service.rs        # RSS 元数据解析、Semantic Scholar/PubMed Abstract 按需补抓
├── settings_service.rs       # DeepSeek 设置读写
└── translate_service.rs      # DeepSeek Chat Completions 请求与错误归一化
```

`article_service.rs` 负责区分 ScienceDirect RSS 元数据和真正 Abstract。RSS 里的 publication date/source/authors 不应作为摘要展示。

## 文档：`docs/`

```text
docs/
├── project-structure.md      # 当前文件结构和职责说明
├── architecture.md           # 架构、数据流、数据库和 command 契约
├── roadmap.md                # MVP 路线、Post-MVP 排除清单
├── design.md                 # UI 设计规范
└── prompts.md                # DeepSeek 翻译 prompt 设计
```

## 构建产物

```text
src-tauri/target/release/bundle/macos/Cento.app
src-tauri/target/release/bundle/dmg/Cento_0.1.0_aarch64.dmg
```

图标由 `scripts/build-icon.sh` 从 `src-tauri/icons/cento.svg` 生成。脚本依赖 `rsvg-convert` 和 macOS `iconutil`。
