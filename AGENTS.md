# AGENTS.md

这个文件给参与 Cento 开发的 AI agent 使用。README 面向人类用户；AGENTS 只记录执行任务时的基本原则。

## 基本原则

- 先读当前代码和文档，再改文件。不要凭旧交接或记忆直接动手。
- 修改前确认任务边界，优先做小而确定的改动。
- 不主动弹出 app 页面；需要视觉验证时，等主要修改完成后再统一打开或说明如何验证。
- 不破坏用户已有改动。遇到未提交改动时先识别来源，能共存就共存。
- Rust 异步 command 里不要让 SQLite `MutexGuard` 跨越 `.await`。
- 翻译相关逻辑必须保持缓存优先，但空字符串不能视为有效缓存。
- ScienceDirect RSS 的 `<description>` 通常只是 publication date/source/authors，不等同于 Abstract。
- 医学和生命科学期刊优先建议使用 PubMed 生成的 RSS；PubMed RSS 往往直接提供完整 Abstract。
- Abstract 外部兜底保持轻量：Semantic Scholar 优先，PubMed 其次，失败后提示用户打开原文。
- 新文章默认未读；点开文章时标记已读。翻译由后台管线在抓取后自动触发，不需要用户点击「翻译全部」（旧 UX 已废弃）。
- 默认 DeepSeek 模型使用 `deepseek-v4-flash`，除非用户明确要求切换。
- 不要重新启用透明窗口、overlay titlebar 或 vibrancy，除非先确认鼠标拖动窗口不会失效。
- 磨玻璃质感全部用 CSS `backdrop-filter` 和半透明叠层在窗口内部模拟，不动 `tauri.conf.json` 的窗口透明设置。
- 强调色默认使用 Claude 赤陶 `#C76547`（变量 `--accent`），未读点、选中条、主按钮、徽章都用同一色系。不要回到 macOS Blue。用户可在设置中切换到海蓝/森绿/墨夜，这由 `[data-accent]` 属性驱动。
- 视觉规范以 `docs/design.md` 为准；Vanilla JS 实现以当前 `src/` 为准。
- 主题、强调色、字号通过 `<body>` 上的 `data-theme` / `data-accent` / `data-font-scale` 三层属性驱动；这些值存在 localStorage，不进 SQLite。
- 客户端持久化（不动 Rust）：`feed-emoji-{id}` / `starred-ids` / `cost-YYYY-MM` / `sidebar-collapsed` / 主题三件套都在 localStorage。如果用户提出要跨机器同步再考虑迁到 DB。
- 三栏宽度用 `clamp()` 响应式，不要回到固定 px。
- 不要再用 floating absolute sidebar；现在是 flex 子项 flush 布局，collapse 用 `margin-left` 负值。
- 每次大改 CSS 时检查文件行数和 `grep -n "^.sidebar {" src/styles.css` 类似命令，确保没有残留旧规则在 cascade 末尾覆盖新规则。

## 任务完成后

- 至少运行 `cargo check` 和 `node --check src/main.js`，如果无法运行要说明原因。
