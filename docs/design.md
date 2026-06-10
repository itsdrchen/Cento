# UI 设计规范

> **视觉方向**：「暖米 + Claude 赤陶 + macOS native」，已落地到 `src/styles.css` + `src/index.html` + `src/main.js`。
>
> **三栏响应式**：sidebar `clamp(220px, 22vw, 300px)`、entry-list `clamp(340px, 32vw, 460px)`、detail-panel `flex: 1 min-width: 360px`。不要回到固定 px 宽。
>
> **主题驱动**：`<body>` 上的 `data-theme` / `data-accent` / `data-font-scale` 三层属性级联控制整个主题系统。`[data-theme="dark"]` 切换深色，`[data-accent]` 支持 `coral` / `blue` / `forest` / `ink`，`[data-font-scale]` 支持 `sm` / `md` / `lg`。
>
> **永久红线**：不重启 transparent window / overlay titlebar / vibrancy。2026-05-25 试过会破坏窗口拖动，已回滚。磨玻璃质感全部用 CSS `backdrop-filter` 在窗口内部模拟。

## 视觉哲学

参考 Apple Books、Apple Mail、Reeder 5。**少即是多，内容优先，UI 退后**。

- **好的设计是隐形的**：用户注意到 UI 就说明它在干扰阅读。
- **留白比分割线更有力**：用间距区分层级，少画线。
- **暖而不腻**：暖米色基调让长时间阅读有温度，但赤陶色只在必要的点用（未读点、选中条、主按钮、徽章）——不要让暖色泛滥到吞掉内容。
- **中文字体优先**：界面语言是中文，排版必须以中文字体为核心调优。
- **双轨字体**：UI chrome 用无衬线，详情区文章正文用衬线。两套字共存就是 Apple Books 的做法。

## Logo

Cento 的 App Icon 是 **macOS squircle 容器 + 衬线 C**。三个候选放在 `design-preview/logo-*.svg`，最终选定后再决定。

| 候选 | 文件 | 描述 |
|---|---|---|
| 墨夜 Ink C（推荐） | `design-preview/logo-ink.svg` | 深棕墨色 squircle + 米色衬线 C。最像 Apple Books / Mail 的气质。 |
| 米纸 Cream C | `design-preview/logo-cream.svg` | 旧纸米黄 squircle + 赤陶色 C。最贴「拾英」名字。 |
| 赤陶 Coral C | `design-preview/logo-coral.svg` | 赤陶色 squircle + 米色 C。Claude 品牌色拉满，存在感最强。 |

设计要点（所有候选共用）：

- **真 squircle，不是圆角矩形**。SVG path 用连续曲率 Bezier 控制点，不是 `rx` 圆角。
- **衬线 C 字形栈**：`Hoefler Text, Baskerville, Garamond, Didot, serif`。生产打包前要把 text 转 outline path（避免不同机器字体替换）。
- **顶部高光环**：squircle 上半圈一条 2px 暖光，模拟 macOS 原生 icon 的内打光。
- **生成流程**：跑 `scripts/build-icon.sh`，会从选定 SVG 生成 PNG、`.iconset`、`cento.icns`。`tauri.conf.json` 的 `bundle.icon` 已经指向 `src-tauri/icons/`，不需要改配置。

## 配色

### 浅色模式（主用）

#### 表面 Surfaces

| Token | 色值 | 用途 |
|---|---|---|
| `--bg-window` | `#F5F0E3` | 窗口背景（暖米） |
| `--bg-titlebar-1` | `#EFE9D9` | 标题栏顶部（渐变） |
| `--bg-titlebar-2` | `#E5DDC6` | 标题栏底部（渐变） |
| `--bg-sidebar` | `#ECE4D0` | 侧栏（比窗口深一档） |
| `--bg-list` | `#F8F2E2` | 文章列表（比窗口亮） |
| `--bg-detail` | `#FBF6E8` | 详情面板（最亮，阅读区） |
| `--bg-card` | `#FFFFFF` | 选中卡片 |
| `--bg-card-selected` | `#FBF0DA` | 高亮态 |

**关键规则**：三栏背景必须做梯度差，sidebar < list < detail，亮度递增。视线自然从左走到右。

#### 文本 Text

| Token | 色值 | 用途 |
|---|---|---|
| `--text-primary` | `#1F1A14` | 主文字 |
| `--text-secondary` | `#6F6757` | 次文字（来源、时间） |
| `--text-tertiary` | `#A39885` | 弱文字（label、提示） |
| `--text-meta` | `#BDB29C` | 元数据 |
| `--text-body` | `#2A2418` | 摘要正文（比主文字略浅，配合衬线字降低疲劳） |

#### 强调色 Accent · Claude Coral

| Token | 色值 | 用途 |
|---|---|---|
| `--accent` | `#C76547` | 主强调色 |
| `--accent-soft` | `#DA7B5B` | 渐变高光端 |
| `--accent-deep` | `#9F4A30` | 文字态强调 |
| `--accent-tint` | `rgba(199,101,71,0.13)` | 选中态背景叠加 |
| `--accent-faint` | `rgba(199,101,71,0.08)` | 未读点光晕、按钮 hover |

**强调色统一用赤陶**，不再用 macOS Blue `#007AFF`。未读小圆点、选中态左侧 3px 边条、主按钮、徽章、deep link 全部用这一套。

#### 边框 Borders

| Token | 色值 | 用途 |
|---|---|---|
| `--border-subtle` | `rgba(50,35,15,0.07)` | 默认 |
| `--border-soft` | `rgba(50,35,15,0.12)` | 强调态、focus ring |
| `--border-hair` | `rgba(50,35,15,0.05)` | 极淡分隔 |

### 暗色模式

暗色 token 见 `design-preview/index.html` 顶部 CSS 变量（待实施）。原则：深棕黑底（不是纯黑）+ 暖米色文字 + 略偏暖的赤陶。

## 字体

### UI chrome（无衬线）

```css
font-family: -apple-system, BlinkMacSystemFont,
             "SF Pro Text", "SF Pro Display",
             "Helvetica Neue",
             "PingFang SC", "Hiragino Sans GB", sans-serif;
```

用于：标题栏、侧栏、文章列表、按钮、徽章、元数据。

### 详情区文章正文（衬线）

```css
font-family: "Source Serif 4", "Noto Serif SC",
             "Songti SC", "Source Han Serif SC",
             Georgia, serif;
```

用于：详情区文章标题、原文标题、摘要正文。`Source Serif 4` 是 Adobe 开源、与中文 `Noto Serif SC` 协调度高。生产打包可考虑本地 woff2 字体子集化，避免网络拉取。

### 等宽

```css
font-family: "SF Mono", "Menlo", "Monaco", monospace;
```

用于：doi、URL、debug 信息。

### 字号 / 字重 / 行高

| 用途 | 字号 | 字重 | 行高 |
|---|---|---|---|
| 窗口标题（titlebar） | 13px | 500 | 1.3 |
| 文章列表标题 | 13.5px | 500 | 1.5 |
| 已读文章标题 | 13.5px | 400 | 1.5 |
| 详情区中文标题 | 26px serif | 600 | 1.38 |
| 详情区英文原标题 | 14.5px serif italic | 400 | 1.55 |
| 摘要正文 | 15.5px serif | 400 | 1.85 |
| 来源 / 日期 | 11.5–12.5px | 400 | 1.3 |
| 按钮 | 12.5–13px | 500 | 1.2 |
| 徽章 / pill | 11px | 500 | 1.2 |

## 布局

### 三栏

```
┌────────────┬────────────────────┬──────────────────────────┐
│  侧栏      │   文章列表          │    详情面板               │
│  240px     │   400px             │    弹性，最小 400px       │
│  最暖深    │   中间              │    最亮，阅读区           │
│  ECE4D0    │   F8F2E2            │    FBF6E8                 │
└────────────┴────────────────────┴──────────────────────────┘
```

- 窗口最小尺寸：900 × 600 px
- 默认尺寸：1200 × 750 px
- 侧栏固定 240–244 px
- 中间栏固定 400 px（列表卡片需要稳定宽度）
- 详情面板弹性，内部 `max-width: 760px` 限制行宽

### 磨玻璃模拟

在窗口内部用 CSS 模拟 macOS vibrancy 效果，**不要动 `tauri.conf.json` 的窗口透明设置**：

```css
.sidebar {
  background: var(--bg-sidebar);
  position: relative;
}
.sidebar::before {
  content: '';
  position: absolute; inset: 0;
  background: linear-gradient(180deg,
    rgba(255,255,255,0.4) 0%,
    rgba(255,255,255,0) 25%);
  pointer-events: none;
}
```

对于 popover、设置面板等浮层，再叠加 `backdrop-filter: blur(20px) saturate(140%)`，前提是父容器有色彩可被模糊。窗口本体保持不透明，鼠标拖动行为不受影响。

## 组件

### 按钮（三档）

| 类型 | 用途 | 样式 |
|---|---|---|
| Primary | 主 CTA（添加、在浏览器打开） | 赤陶渐变 + 白字 + 软投影 |
| Secondary | 次操作（重新翻译） | 半透明白底 + 灰边 + 深文字 |
| Ghost | 列表头次级操作 | 米色底 + 赤陶字 + hover 变浅赤陶背景 |

Primary 按钮配方：

```css
background: linear-gradient(180deg, var(--accent-soft) 0%, var(--accent) 100%);
color: white;
border: none; border-radius: 8px;
box-shadow:
  0 1px 0 rgba(255,255,255,0.25) inset,
  0 2px 6px rgba(199,101,71,0.32),
  0 0 0 0.5px rgba(159,74,48,0.4);
```

### 列表项 / 文章卡片

- 默认透明背景，padding `12px 14px`
- Hover：`rgba(0,0,0,0.025)` 叠加
- 选中：白底 + `--shadow-card` + 左侧 `-1px` 处 3px 赤陶条
- 未读：标题字重 500、左侧 7px 赤陶圆点 + 软光晕；已读：字重 400、文字降到 secondary、圆点透明

### 徽章 Badge

赤陶 tint 背景 + 赤陶深色文字，前置 4px 圆点：

```css
.badge {
  display: inline-flex; align-items: center; gap: 5px;
  padding: 3px 9px;
  background: var(--accent-faint);
  color: var(--accent-deep);
  border-radius: 11px;
  font-size: 11px; font-weight: 500;
}
.badge::before {
  content: ''; width: 4px; height: 4px; border-radius: 50%;
  background: var(--accent);
}
```

用于：摘要来源（PubMed / Semantic Scholar）、未读状态。

### 输入框

- 圆角 7px
- 边框 `0.5px solid var(--border-soft)`
- 半透明白底 `rgba(255,255,255,0.7)`
- 内阴影 `inset 0 1px 1px rgba(40,25,10,0.04)`（轻微凹陷感）
- Focus：边框变 `--accent`，外侧 3px `--accent-faint` 光晕

### Traffic light（标题栏）

如果以后用 overlay titlebar 想自绘 traffic light，**先确认窗口拖动不会失效**。在那之前继续用 macOS 系统 titlebar。

## 间距 / 圆角 / 阴影

### 间距（4px 栅格）

| Token | 值 | 用途 |
|---|---|---|
| `--space-xs` | 4px | 紧密关联 |
| `--space-sm` | 8px | 标签与值 |
| `--space-md` | 12px | 列表内边距 |
| `--space-lg` | 16px | 卡片内边距 |
| `--space-xl` | 24px | 面板内边距 |
| `--space-2xl` | 32px | 区块间距 |
| `--space-3xl` | 44–52px | 详情区横向内边距 |

### 圆角

| Token | 值 | 用途 |
|---|---|---|
| `--radius-sm` | 6–7px | 输入框、小按钮 |
| `--radius-md` | 8–9px | 卡片、按钮 |
| `--radius-lg` | 11px | pill、徽章 |
| `--radius-window` | 12–14px | 窗口边角 |

### 阴影

```css
--shadow-card: 0 1px 2px rgba(40,25,10,0.05),
               0 0 0 0.5px rgba(50,35,15,0.07);
--shadow-card-hover: 0 2px 8px rgba(40,25,10,0.08),
                     0 0 0 0.5px rgba(50,35,15,0.10);
--shadow-accent-button: 0 1px 0 rgba(255,255,255,0.25) inset,
                        0 2px 6px rgba(199,101,71,0.32),
                        0 0 0 0.5px rgba(159,74,48,0.40);
```

macOS 阴影风格：低透明度 + 大模糊 + 0.5px hairline 描边模拟 retina 描边。

## 动画

- 过渡动画 < 200ms（hover、selected 切换）
- 功能性动画可以更长（刷新旋转、状态变化提示）
- 不做装饰性动画
- `prefers-reduced-motion` 时关闭非必要 transition

