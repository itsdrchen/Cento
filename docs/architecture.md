# 架构说明

Cento 是一个 Tauri 2 桌面应用：前端负责界面和交互，Rust 后端负责 RSS、SQLite、翻译和系统能力调用。

## 分层

```text
前端 HTML/CSS/JS
  │
  └─ window.__TAURI__.core.invoke(...)
       │
       ▼
Tauri commands
  │
  └─ 调用 services
       │
       ├─ SQLite 本地存储
       ├─ RSS/Atom HTTP 抓取
       ├─ DeepSeek API 翻译
       └─ Abstract 补抓、来源标记与元数据解析
```

## 前端

- `src/index.html`：设置页、工具栏、三栏主界面的 DOM 骨架。
- `src/main.js`：应用状态、事件绑定、列表渲染、详情渲染、Tauri command 调用。
- `src/styles.css`：macOS 风格布局、色彩、深色模式、组件样式。

前端使用原生 JavaScript，不引入框架。所有后端能力通过 `invoke("command_name", args)` 调用。

## 后端

- `commands/` 是 Tauri command 边界。职责是接收前端参数、获取 `DbState`、调用 service。
- `services/` 是业务逻辑层。职责是 RSS 抓取、数据库读写、翻译请求、Abstract 补抓和设置管理。
- `db.rs` 负责 SQLite 初始化和轻量迁移。
- `models.rs` 定义前后端共享的序列化结构。

## SQLite

数据库位置：

```text
~/Library/Application Support/io.github.itsdrchen.cento/cento.db
```

### `feeds`

订阅源表。

- `id`：主键
- `url`：RSS URL，唯一
- `title`：RSS 源标题或用户重命名后的标题
- `description`：RSS 源描述
- `created_at`：创建时间

### `entries`

文章条目表。

- `id`：主键
- `feed_id`：所属订阅源
- `guid`：RSS guid，用于去重
- `title`：原文标题
- `link`：原文 URL
- `summary`：RSS 原始摘要或公共索引补抓到的 Abstract；纯 publication date/source/authors 元数据不会作为摘要展示
- `summary_source`：摘要来源，常见值为 `rss`、`semantic_scholar`、`pubmed`
- `author`：RSS 作者字段
- `published_at`：RSS 发布时间
- `publication_date`：从 RSS description 拆出的 publication date
- `source`：从 RSS description 拆出的 source/journal
- `fetched_at`：抓取时间
- `is_read`：已读状态，新文章默认 `0`
- `read_at`：首次标记为已读的时间

### `translations`

翻译缓存表。

- `entry_id`：文章条目
- `field`：`title` 或 `summary`
- `original_text`：翻译时的原文
- `translated_text`：翻译结果
- `model`：使用的模型
- `created_at`：翻译时间

空字符串不能视为有效翻译缓存。

### `settings`

键值配置表。

- DeepSeek API Key
- DeepSeek Base URL
- DeepSeek Model，默认 `deepseek-v4-flash`
- System Prompt

## 核心流程

### 订阅源选择

医学和生命科学期刊优先使用 PubMed 生成的 RSS，而不是出版社官网 RSS。PubMed RSS 的 `<description>` 通常包含完整 Abstract；出版社 RSS，尤其是 ScienceDirect，常只给 publication date/source/authors。

PubMed RSS URL 由页面的 “Create RSS” 生成，包含服务器 hash，不能可靠手写拼接。新增期刊时优先在 PubMed 用 `期刊名或 NLM 缩写[Journal]` 搜索，例如 `Phytomedicine[Journal]`、`J Ethnopharmacol[Journal]`。

### 刷新 RSS

```text
点击刷新
  → fetch_all_feeds
  → 拉取每个 RSS URL
  → feed-rs 解析 entries
  → 按 feed_id + guid 去重
  → 拆分 publication date/source
  → 写入 entries.summary 与 summary_source = rss
  → 对新标题调用 DeepSeek 翻译
  → 写入 translations
```

刷新负责 RSS 拉取、入库；随后自动触发后台翻译管线，无需用户手动操作。

### 自动翻译管线

`services::translation_pipeline` 是一个 fire-and-forget 的 tokio 任务，每次抓取后自动 spawn：

```text
spawn(app)
  → 查询 entries 中所有未翻译的标题 / 摘要（PENDING_LIMIT = 200）
  → 跳过已读条目
  → 按信号量并发（MAX_CONCURRENT = 3）
  → 标题：translate_text → 写 translations 缓存
  → 摘要：缺摘要时 fetch_abstract → translate_text → 写 translations 缓存
  → 每步通过 `translation-progress` 事件回报前端
  → 管线级状态通过 `translation-status` 事件回报（needs_key / auth_failed / ok）
```

管线 idempotent：DB 查询永远只返回还差翻译的条目，重复 spawn 不会重复翻译。前端通过事件流实时更新列表与详情面板。

### 打开文章详情

```text
点击文章
  → set_entry_read 标记为已读
  → 前端显示标题、publication date、source
  → 若 summary 为空，调用 fetch_abstract
  → 后端尝试 Semantic Scholar，再尝试 PubMed
  → 成功则缓存到 entries.summary，并写入 summary_source
  → 失败则提示暂无摘要，用户可打开原文
```

### 已读 / 未读

```text
新入库文章
  → is_read = 0
  → 列表显示未读圆点和更高字重

点击文章
  → 标记 is_read = 1
  → read_at 写入首次阅读时间

右键文章
  → 可切换标为已读 / 标为未读
```

### 翻译

```text
翻译请求
  → 先查 translations 非空缓存
  → 无缓存则调用 DeepSeek
  → 空结果视为失败
  → 成功写入 translations
```

## 异步与 SQLite 锁

`DbState` 使用 `Mutex<Connection>` 管理 SQLite 连接。异步 command 必须遵守：

```rust
let data = {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    // 只做同步 DB 读取
    data
}; // 在 await 前释放锁

external_request(data).await?;
```

不要让 `MutexGuard` 跨越 `.await`。

## Command 契约

### 订阅

- `add_feed(url)`
- `list_feeds()`
- `delete_feed(id)`
- `rename_feed(id, name)`

### 抓取

- `fetch_all_feeds()`

### 文章

- `list_entries(feedId)`
- `fetch_abstract(entryId)`
- `set_entry_read(entryId, isRead)`

### 设置

- `get_settings()`
- `save_settings(settings)`
- `test_connection(settings)`

### 翻译与原文

- `translate_title(entryId)`
- `translate_summary(entryId)`
- `open_url(url)`

## 打包与桌面版

```bash
npm run build
```

构建后 Tauri 生成 macOS App 与 dmg：

```text
src-tauri/target/release/bundle/macos/Cento.app
src-tauri/target/release/bundle/dmg/Cento_0.1.0_aarch64.dmg
```

窗口使用标准 macOS titlebar。不要默认启用透明窗口、overlay titlebar 或 vibrancy；之前这会导致窗口拖动行为异常。
