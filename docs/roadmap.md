# 路线图

Cento 的产品目标是：帮助中文用户快速判断英文 RSS 内容是否值得打开原文阅读。路线图只围绕这个目标展开。

## 已完成的 MVP

- 订阅管理：添加、展示、重命名、删除 RSS 源。
- 手动刷新：拉取 RSS、解析、去重、写入 SQLite。
- DeepSeek 设置：API Key、Base URL、Model、System Prompt、测试连接。
- 自动翻译：后台管线在抓取后异步翻译标题与摘要，并发受信号量控制，结果缓存到 SQLite。
- 失败可恢复：单条翻译失败显示错误徽章，下次管线重跑时会重新尝试。
- 摘要补抓：RSS 摘要缺失时按需调用 Semantic Scholar / PubMed 补抓 Abstract。
- 已读/未读：新文章默认未读，点开后自动标已读，右键可切回未读。
- RSS 元数据处理：description 中的 publication date/source/authors 与真正 Abstract 分离展示。
- Abstract 补抓：按需尝试 Semantic Scholar / PubMed，并缓存 Abstract 与来源。
- 打开原文：调用系统浏览器打开文章链接。
- 桌面版：已生成 macOS `.app` 和 `.dmg`，可通过 `npm run tauri build` 在 `src-tauri/target/release/bundle/` 下生成。

## 近期优先级

1. 建立高质量订阅源清单：医学和生命科学期刊优先使用 PubMed 生成的 RSS。
2. 优化文章详情页的信息密度：标题、原文标题、publication date、source、Abstract。
3. 保持 Abstract 获取失败可解释，能区分 RSS 无摘要、公共索引未收录、网络限流。
4. 保持翻译失败可解释，避免"假成功"。
5. 后续 UI 调整必须小步验证，尤其避免再次破坏窗口拖动。

## 可以考虑的小功能

- 左侧订阅源显示文章数。
- 键盘快捷键：刷新、打开设置。
- 更清晰地区分“已翻译标题”和“原文标题”。
- 在摘要旁显示来源角标：`RSS` / `Semantic Scholar` / `PubMed`。
- 设置页增加模型下拉项：`deepseek-v4-flash` / `deepseek-v4-pro`。

## Post-MVP 排除清单

以下功能暂不做，除非它们明确服务于“更快判断是否值得读原文”。

### 收藏 / 稍后阅读

不做。保存和整理内容属于笔记工具、稍后读工具或 Zotero。

### 分类文件夹

暂不做。MVP 假设用户只订阅少量高价值源，少一层分类就少一次决策。

### OPML 导入导出

暂不做。手动添加少量 RSS URL 的成本低于维护导入导出链路。

### 自动定时刷新

不做。自动刷新会在后台持续消耗翻译 API 额度。Cento 保持用户手动触发。

### 菜单栏常驻

不做。Cento 是阅读筛选工具，不是状态监控工具。

### Obsidian / Notion / Readwise 集成

不做。Cento 不进入知识库工作流。

### 全文抓取

不做。全文抓取成本高、边界复杂，也偏离“先判断是否值得读”的目标。

### PDF 下载

不做。PDF 获取和管理交给浏览器或 Zotero。

### 影响因子查询

不做。它属于文献管理和评价系统，不属于轻量 RSS 阅读器核心。

### 推荐算法

不做。Cento 的哲学是用户主动选择订阅源，而不是算法推荐。
