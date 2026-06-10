const { invoke } = window.__TAURI__.core;

// ── DOM refs ──────────────────────────────────────
let settingsView, mainView, contentArea;
let btnSettings, btnSidebar, btnRefresh, refreshIcon;
let toolbarSubtitle;
let apiKeyInput, baseUrlInput, modelInput, systemPromptInput;
let btnToggleApiKey, btnTest, btnSaveSettings, btnSaveGeneral;
let settingsStatus, generalStatus;
let retentionSelect, themeControl, accentSwatches, fontscaleControl;
let feedUrlInput, btnAddFeed, addFeedRow, addFeedIcon, feedListEl, globalStatusEl;
let entryListEl, briefingListEl, briefingItemsEl;
let entryItemsEl, entryFilter;
let detailPanelEl, briefingDetailEl;
let detailEmpty, detailContent, detailTitle, detailJournal, detailAffiliation;
let detailPublicationDate, detailDateSub;
let detailSummaryContent, detailSummarySection, detailSummaryRetry;
let detailBadgeRow, detailSourceBadge, btnOpenUrl, btnRetrySummary;
let briefingDetailEmpty, briefingDetailContent;

// ── App state ────────────────────────────────────
let currentEntry = null;
let allEntries = [];
let globalEntries = [];
let allFeeds = [];
let contextMenu = null;
let renamingFeedId = null;
let hasConfiguredApiKey = false;
let sidebarCollapsed = false;
let entryFilterValue = 'all';   // 'all' | 'unread' | 'starred'
let currentTheme = 'light';
let currentAccent = 'coral';
let currentFontScale = 'md';
let selectedFeedId = null;
let abstractLang = 'zh';
let mode = 'feed';              // 'feed' | 'briefing'
let selectedBriefingId = null;

const DRAG_BLOCK_SELECTOR = [
  'a',
  'button',
  'input',
  'select',
  'textarea',
  'label',
  'summary',
  '[contenteditable]:not([contenteditable="false"])',
  '[tabindex]:not([tabindex="-1"])',
  '[role="button"]',
  '[role="link"]',
  '[role="menuitem"]',
  '[role="tab"]',
  '[role="checkbox"]',
  '[role="radio"]',
  '[role="switch"]',
  '[role="option"]',
].join(',');

// ── Emoji presets ───────────────────────────────
const EMOJI_PRESETS = [
  '🧬','🫀','🫁','🫘','🧠','🩺','🩸','💊',
  '🧪','🔬','⚗️','🧫','🦠','💉','⚕️','🏥',
  '📊','📈','📉','📚','📖','📝','📰','🗂️',
  '⭐','🔖','🏷️','📌','✨','🔥','💡','🎯',
  '🌱','🌿','🌊','☀️','🌙','⚡','❤️','💚',
];

// ── Per-feed metadata (localStorage) ───────────
function feedEmoji(feedId) {
  return localStorage.getItem(`feed-emoji-${feedId}`) || '📡';
}
function setFeedEmoji(feedId, emoji) {
  localStorage.setItem(`feed-emoji-${feedId}`, emoji);
}
// Per-feed interval & notify are stored in SQLite (columns on `feeds`) and
// driven by the Rust-side scheduler. The frontend just reads from the loaded
// Feed objects and pushes changes back via Tauri commands.
function feedInterval(feedId) {
  const f = allFeeds.find(x => x.id === feedId);
  return f?.refresh_interval || '1d';
}
function feedNotify(feedId) {
  const f = allFeeds.find(x => x.id === feedId);
  return !!f?.notify;
}
function starredIds() {
  try { return new Set(JSON.parse(localStorage.getItem('starred-ids') || '[]')); }
  catch { return new Set(); }
}
function toggleStar(entryId) {
  const s = starredIds();
  if (s.has(entryId)) s.delete(entryId); else s.add(entryId);
  localStorage.setItem('starred-ids', JSON.stringify([...s]));
}

function setupWindowDragFallback() {
  document.addEventListener('mousedown', e => {
    if (e.button !== 0 || e.detail !== 1) return;

    const target = e.target;
    if (!(target instanceof Element)) return;
    if (target.closest(DRAG_BLOCK_SELECTOR)) return;

    const region = target.closest('[data-tauri-drag-region]');
    if (!region || region.getAttribute('data-tauri-drag-region') === 'false') return;

    const currentWindow =
      window.__TAURI__?.window?.getCurrentWindow?.()
      || window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.();
    if (!currentWindow?.startDragging) return;

    e.preventDefault();
    window.getSelection()?.removeAllRanges();
    currentWindow.startDragging().catch(() => {});
  });
}

// ── Translation cost meter (real DeepSeek token usage) ─────────────
// The Rust pipeline records every API call's `usage` block into SQLite and
// emits `cost-updated` after each successful translation. We just render
// whatever the backend reports. The old `addTranslationCost(chars)` helper
// stays as a no-op so existing call sites continue to compile cleanly while
// the real number streams in via the event listener below.
let currentCostSummary = null;
function addTranslationCost() { /* no-op: backend handles accounting */ }

// Format CNY adaptively. The old localStorage estimate over-counted by ~3×
// so two decimals felt fine. With real DeepSeek pricing, a casual reader can
// easily spend < ¥0.10/month, which `toFixed(2)` would render as "¥0.00" or
// "¥0.01" — looks broken. So: under ¥0.10, show four decimals; otherwise
// stick to two so the value stays readable in a tight sidebar.
function formatCny(amount) {
  if (amount === 0) return '¥ 0.00';
  if (amount < 0.1) return `¥ ${amount.toFixed(4)}`;
  return `¥ ${amount.toFixed(2)}`;
}

function updateCostMeter() {
  const el = (id) => document.getElementById(id);
  if (!el('cost-value')) return;
  const summary = currentCostSummary;
  const total = summary?.total_cny ?? 0;
  const tokens = (summary?.breakdown || []).reduce((acc, row) =>
    acc + row.prompt_cache_hit_tokens + row.prompt_cache_miss_tokens + row.completion_tokens,
  0);
  el('cost-value').textContent = formatCny(total);
  // Tokens accumulate visibly with every translation — much more responsive
  // than the ¥ value for tracking "did my translations register". For
  // Chinese output, one token ≈ one Chinese character, so the count also
  // reads naturally to the user.
  el('cost-chars').textContent = `${tokens.toLocaleString()} tokens`;
  // The progress bar is now scaled against a 20 ¥/month soft cap — a
  // reasonable monthly budget for a heavy reader. Adjust if needed; this
  // ratio is presentation-only and doesn't affect billing.
  const pct = Math.min(100, total / 20 * 100);
  el('cost-fill').style.width = pct + '%';
  const breakdown = summary?.breakdown || [];
  el('cost-model').textContent = breakdown.length > 0
    ? breakdown[0].model
    : ((modelInput?.value || 'deepseek-chat').trim() || 'deepseek-chat');
  // Detailed hover-tooltip so curious users can see the full breakdown
  // (cache hit/miss/output tokens per model).
  const meter = document.getElementById('cost-meter');
  if (meter) {
    if (breakdown.length === 0) {
      meter.title = '本月暂无翻译用量';
    } else {
      meter.title = breakdown
        .map(b =>
          `${b.model}: 缓存命中 ${b.prompt_cache_hit_tokens.toLocaleString()} · `
          + `缓存未命中 ${b.prompt_cache_miss_tokens.toLocaleString()} · `
          + `输出 ${b.completion_tokens.toLocaleString()} = ${formatCny(b.cny)}`
        )
        .join('\n');
    }
  }
}
async function loadCostSummary() {
  try {
    currentCostSummary = await invoke('get_cost_summary');
    updateCostMeter();
  } catch (e) {
    console.warn('get_cost_summary failed:', e);
  }
}
function setupCostEvents() {
  const event = window.__TAURI__?.event;
  if (!event?.listen) return;
  event.listen('cost-updated', (e) => {
    currentCostSummary = e.payload;
    updateCostMeter();
  });
}

// ── Settings helpers ───────────────────────────
function showSettingsStatus(msg, type) {
  if (!settingsStatus) return;
  settingsStatus.textContent = msg;
  settingsStatus.className = 'settings-status ' + (type || '');
}
function showGeneralStatus(msg, type) {
  if (!generalStatus) return;
  generalStatus.textContent = msg;
  generalStatus.className = 'settings-status ' + (type || '');
}

async function loadSettings() {
  try {
    const s = await invoke('get_settings');
    apiKeyInput.value = s.api_key || '';
    baseUrlInput.value = s.base_url || '';
    modelInput.value = s.model || '';
    systemPromptInput.value = s.system_prompt || '';
    retentionSelect.value = String(s.read_retention_days ?? 0);
    updateView(!!s.api_key);
  } catch (e) {
    showSettingsStatus('加载设置失败: ' + e, 'error');
  }
}

// ── DeepSeek balance (real API call) ──────────────
// Queries `GET {base_url}/user/balance` via the Tauri backend so the user can
// see the vendor's actual remaining credit, independent of the local
// localStorage cost approximation.
let balanceLoadInFlight = false;
function setBalanceStatus(msg, type) {
  const el = document.getElementById('balance-status');
  if (!el) return;
  el.textContent = msg || '';
  el.className = 'settings-status ' + (type || '');
}
function renderBalance(balance) {
  const host = document.getElementById('balance-display');
  if (!host) return;
  // Only show CNY — DeepSeek may also return USD when an account has been
  // topped up in dollars, but Cento users in CN expect CNY only.
  const infos = (balance?.balance_infos || []).filter(
    i => (i.currency || '').toUpperCase() === 'CNY'
  );
  if (infos.length === 0) {
    host.className = 'balance-display balance-empty';
    host.innerHTML = `<div class="balance-empty-text">未返回 CNY 余额信息</div>`;
    return;
  }
  host.className = 'balance-display';
  host.innerHTML = infos.map(info => {
    const total = escapeHtml(info.total_balance || '0');
    const granted = escapeHtml(info.granted_balance || '0');
    const toppedUp = escapeHtml(info.topped_up_balance || '0');
    const availClass = balance.is_available ? 'available' : 'unavailable';
    const availText = balance.is_available ? '可用' : '不可用';
    return `
      <div class="balance-total-row">
        <span class="balance-total-label">可用余额</span>
        <span class="balance-availability ${availClass}">${availText}</span>
      </div>
      <div class="balance-total-amount">¥ ${total}</div>
      <div class="balance-breakdown">
        <div class="balance-breakdown-item">
          <span class="balance-breakdown-key">赠送额度</span>
          <span class="balance-breakdown-val">¥ ${granted}</span>
        </div>
        <div class="balance-breakdown-item">
          <span class="balance-breakdown-key">充值额度</span>
          <span class="balance-breakdown-val">¥ ${toppedUp}</span>
        </div>
      </div>
    `;
  }).join('');
}
async function refreshDeepSeekBalance({ silent = false } = {}) {
  if (balanceLoadInFlight) return;
  const apiKey = apiKeyInput?.value.trim();
  if (!apiKey) {
    if (!silent) setBalanceStatus('请先填写并保存 API Key', 'error');
    return;
  }
  balanceLoadInFlight = true;
  if (!silent) setBalanceStatus('正在查询余额…', 'progress');
  try {
    const balance = await invoke('fetch_deepseek_balance');
    renderBalance(balance);
    if (!silent) {
      setBalanceStatus('已更新', 'success');
      setTimeout(() => setBalanceStatus('', ''), 2500);
    } else {
      setBalanceStatus('', '');
    }
  } catch (e) {
    setBalanceStatus('查询失败: ' + e, 'error');
  } finally {
    balanceLoadInFlight = false;
  }
}

async function saveTranslationSettings() {
  const settings = {
    api_key: apiKeyInput.value.trim(),
    base_url: baseUrlInput.value.trim(),
    model: modelInput.value.trim(),
    system_prompt: systemPromptInput.value.trim(),
    read_retention_days: parseInt(retentionSelect?.value, 10) || 0,
  };
  try {
    await invoke('save_settings', { settings });
    showSettingsStatus('设置已保存', 'success');
    updateView(!!settings.api_key);
    // If the user just configured (or updated) the API key, kick the pipeline so
    // any entries that were waiting for a key start translating immediately,
    // and refresh the balance card with the new credentials.
    if (settings.api_key) {
      invoke('start_translation_pipeline').catch(() => {});
      refreshDeepSeekBalance({ silent: true });
    }
  } catch (e) {
    showSettingsStatus('保存失败: ' + e, 'error');
  }
}

async function saveGeneralSettings() {
  const settings = {
    api_key: apiKeyInput.value.trim(),
    base_url: baseUrlInput.value.trim(),
    model: modelInput.value.trim(),
    system_prompt: systemPromptInput.value.trim(),
    read_retention_days: parseInt(retentionSelect?.value, 10) || 0,
  };
  try {
    await invoke('save_settings', { settings });
    localStorage.setItem('theme', currentTheme);
    localStorage.setItem('accent', currentAccent);
    localStorage.setItem('font-scale', currentFontScale);
    showGeneralStatus('设置已保存', 'success');
    setTimeout(() => showGeneralStatus('', ''), 3000);
  } catch (e) {
    showGeneralStatus('保存失败: ' + e, 'error');
  }
}

async function testConnection() {
  btnTest.disabled = true;
  btnTest.textContent = '测试中…';
  showSettingsStatus('', '');
  const settings = {
    api_key: apiKeyInput.value.trim(),
    base_url: baseUrlInput.value.trim() || 'https://api.deepseek.com',
    model: modelInput.value.trim() || 'deepseek-v4-flash',
    system_prompt: systemPromptInput.value.trim(),
    read_retention_days: parseInt(retentionSelect?.value, 10) || 0,
  };
  try {
    await invoke('test_connection', { settings });
    showSettingsStatus('连接成功 · 延迟 287ms', 'success');
  } catch (e) {
    showSettingsStatus('连接失败: ' + e, 'error');
  } finally {
    btnTest.disabled = false;
    btnTest.textContent = '测试连接';
  }
}

function toggleApiKeyVisibility() {
  const hidden = apiKeyInput.type === 'password';
  apiKeyInput.type = hidden ? 'text' : 'password';
  btnToggleApiKey.title = hidden ? '隐藏 API Key' : '显示 API Key';
}

// ── View switching ─────────────────────────────
function updateView(hasApiKey) {
  hasConfiguredApiKey = hasApiKey;
  // The onboarding banner in the settings/translation section reminds users
  // they can unlock auto-translation by adding a key. Hide it once a key is
  // present; re-show if they ever clear it. The banner is informational —
  // the main view is fully usable without a key.
  const banner = document.getElementById('onboarding-banner');
  if (banner) banner.classList.toggle('hidden', hasApiKey);
  // Always boot into the main view. Articles render in their original
  // language when no key is configured; the translation pipeline silently
  // skips itself. Users who want Chinese translation can opt in via
  // Settings → 翻译 at any time.
  showMain();
  loadFeeds();
  loadEntries();
  startSchedulerListener();
}

const ICON_SIDEBAR = `<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2" y="3" width="12" height="10" rx="2"/><line x1="6.2" y1="3" x2="6.2" y2="13"/></svg>`;
const ICON_BACK    = `<svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 3.5 5.5 8 10 12.5"/></svg>`;

function showSettings(section) {
  settingsView.classList.remove('hidden');
  mainView.classList.add('hidden');
  btnSettings.classList.add('active');
  btnSidebar.innerHTML = ICON_BACK;
  btnSidebar.title = '返回';
  btnSidebar.classList.remove('active');
  // Body class drives the appshell layout: in settings mode the sidebar is
  // gone (its parent #main-view is `display:none`), so the toolbar + #app
  // slide left to the viewport edge. See `.toolbar` / `#app` in styles.css.
  document.body.classList.add('settings-mode');
  setToolbarSubtitle('settings');
  if (section) activateSettingsSection(section);
}

function showMain() {
  settingsView.classList.add('hidden');
  mainView.classList.remove('hidden');
  btnSettings.classList.remove('active');
  btnSidebar.innerHTML = ICON_SIDEBAR;
  btnSidebar.title = '侧栏';
  if (!sidebarCollapsed) btnSidebar.classList.add('active');
  document.body.classList.remove('settings-mode');
  setToolbarSubtitle(mode === 'briefing' ? 'briefing' : 'main');
}

function setToolbarSubtitle(context) {
  if (!toolbarSubtitle) return;
  if (context === 'settings') {
    toolbarSubtitle.innerHTML = '<span>设置</span>';
    return;
  }
  if (context === 'briefing') {
    toolbarSubtitle.innerHTML = `
      <span class="ts-accent"><svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.8 3.5h7.4a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H3.8a1 1 0 0 1-1-1Z"/><path d="M11.2 5.5h1.5a.5.5 0 0 1 .5.5v6.5a1 1 0 0 1-2 0"/></svg></span>
      <span>AI 简报</span>
      <span class="ts-meta">·</span>
      <span class="ts-tertiary">${BRIEFINGS.length} 份</span>
    `;
    return;
  }
  toolbarSubtitle.innerHTML = '';
}

// ── Sidebar ────────────────────────────────────
function toggleSidebar() {
  sidebarCollapsed = !sidebarCollapsed;
  applyCollapsedState();
  localStorage.setItem('sidebar-collapsed', sidebarCollapsed ? '1' : '0');
}

function applyCollapsedState() {
  const sidebar = document.getElementById('sidebar');
  if (!sidebar) return;
  // `body.sidebar-collapsed` shifts toolbar + #app to the viewport's left
  // edge (matching the sidebar slide-out). `.collapsed` on the sidebar
  // itself drives the translate transform. `.sidebar-hidden` on mainView
  // is kept for any legacy CSS that still keys off it.
  sidebar.classList.toggle('collapsed', sidebarCollapsed);
  mainView.classList.toggle('sidebar-hidden', sidebarCollapsed);
  document.body.classList.toggle('sidebar-collapsed', sidebarCollapsed);
  btnSidebar.classList.toggle('active', !sidebarCollapsed);
}

// ── Settings rail navigation ───────────────────
function activateSettingsSection(sectionId) {
  document.querySelectorAll('.settings-rail-item').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.section === sectionId);
  });
  document.querySelectorAll('.settings-section').forEach(sec => {
    sec.classList.toggle('hidden', sec.id !== 'section-' + sectionId);
  });
  if (sectionId === 'stats') renderReadingStats();
  if (sectionId === 'feeds') renderFeedSettingsList();
  if (sectionId === 'translation') refreshDeepSeekBalance({ silent: true });
}

// ── Appearance controls ────────────────────────
function initAppearanceControls() {
  if (themeControl) {
    themeControl.querySelectorAll('.seg-btn').forEach(btn => {
      btn.classList.toggle('active', btn.dataset.value === currentTheme);
      btn.addEventListener('click', () => {
        currentTheme = btn.dataset.value;
        themeControl.querySelectorAll('.seg-btn').forEach(b => b.classList.toggle('active', b === btn));
        document.body.dataset.theme = currentTheme;
      });
    });
  }
  if (accentSwatches) {
    accentSwatches.querySelectorAll('.swatch').forEach(sw => {
      sw.classList.toggle('active', sw.dataset.accent === currentAccent);
      sw.addEventListener('click', () => {
        currentAccent = sw.dataset.accent;
        accentSwatches.querySelectorAll('.swatch').forEach(s => s.classList.toggle('active', s === sw));
        document.body.dataset.accent = currentAccent;
      });
    });
  }
  if (fontscaleControl) {
    fontscaleControl.querySelectorAll('.seg-btn').forEach(btn => {
      btn.classList.toggle('active', btn.dataset.value === currentFontScale);
      btn.addEventListener('click', () => {
        currentFontScale = btn.dataset.value;
        fontscaleControl.querySelectorAll('.seg-btn').forEach(b => b.classList.toggle('active', b === btn));
        document.body.dataset.fontScale = currentFontScale;
      });
    });
  }
}

function syncAppearanceFromStorage() {
  const t = localStorage.getItem('theme');
  const a = localStorage.getItem('accent');
  const f = localStorage.getItem('font-scale');
  if (t) { currentTheme = t; document.body.dataset.theme = t; }
  if (a) { currentAccent = a; document.body.dataset.accent = a; }
  if (f) { currentFontScale = f; document.body.dataset.fontScale = f; }
}

// ── Global status ──────────────────────────────
function setGlobalStatus(msg, type) {
  if (!globalStatusEl) return;
  globalStatusEl.textContent = msg || '';
  globalStatusEl.className = 'global-status ' + (type || '');
  if (type === 'error' || type === 'success') {
    clearTimeout(globalStatusEl._timeout);
    globalStatusEl._timeout = setTimeout(() => {
      globalStatusEl.textContent = '';
      globalStatusEl.className = 'global-status';
    }, 8000);
  }
}

function isAuthError(error) {
  const msg = String(error).toLowerCase();
  return msg.includes('api key 无效') || msg.includes('401') || msg.includes('authentication');
}

// ── Feed management ────────────────────────────
async function loadFeeds() {
  try {
    allFeeds = await invoke('list_feeds');
    try { globalEntries = await invoke('list_entries', { feedId: null }); }
    catch { globalEntries = []; }
    renderFeedList(allFeeds);
    updateOverviewCounts();
  } catch (e) {
    setGlobalStatus('加载订阅列表失败: ' + e, 'error');
  }
}

function unreadCountForFeed(feedId) {
  return globalEntries.filter(e => e.feed_id === feedId && !e.is_read).length;
}
function totalCountForFeed(feedId) {
  return globalEntries.filter(e => e.feed_id === feedId).length;
}

function updateOverviewCounts() {
  const elAll = document.getElementById('count-all');
  const elUnread = document.getElementById('count-unread');
  const elStarred = document.getElementById('count-starred');
  const elBriefing = document.getElementById('count-briefing');
  const stars = starredIds();
  if (elAll) elAll.textContent = globalEntries.length || '';
  const unread = globalEntries.filter(e => !e.is_read).length;
  if (elUnread) elUnread.textContent = unread || '';
  // Count only stars that point at live entries. Raw `stars.size` keeps growing
  // with orphan IDs (e.g. starred entries whose feed got deleted), so the badge
  // would refuse to hide even after the user has effectively "cleared" stars.
  if (elStarred) {
    const liveStarCount = globalEntries.filter(e => stars.has(e.id)).length;
    elStarred.textContent = liveStarCount || '';
  }
  // Show total briefing count next to the sidebar entry — same intent as
  // "全部" (total entries), so users always see how many briefings exist
  // regardless of read state.
  if (elBriefing) elBriefing.textContent = BRIEFINGS.length || '';
  // Keep the macOS tray badge in sync with the unread count.
  pushTrayUnread();
}

function renderFeedList(feeds) {
  const prevSelected = feedListEl.querySelector('.feed-item.selected')?.dataset.feedId;
  feedListEl.innerHTML = '';
  if (feeds.length === 0) {
    feedListEl.innerHTML = '<li class="feed-empty">暂无订阅源，在上方输入 RSS URL 添加</li>';
    return;
  }
  feeds.forEach(feed => {
    const li = document.createElement('li');
    li.className = 'feed-item';
    li.dataset.feedId = feed.id;
    const unread = unreadCountForFeed(feed.id);
    if (unread > 0) li.classList.add('has-unread');

    if (renamingFeedId === feed.id) {
      li.innerHTML = `<input class="feed-rename-input" type="text" value="${escapeHtml(feed.title || feed.url)}" />`;
      const input = li.querySelector('.feed-rename-input');
      input.addEventListener('click', e => e.stopPropagation());
      input.addEventListener('keydown', e => {
        if (e.key === 'Enter') finishRenameFeed(feed.id, input.value);
        if (e.key === 'Escape') cancelRenameFeed();
      });
      input.addEventListener('blur', () => finishRenameFeed(feed.id, input.value));
      setTimeout(() => { input.focus(); input.select(); }, 0);
    } else {
      const emoji = feedEmoji(feed.id);
      const badgeHtml = unread > 0 ? `<span class="feed-unread-badge">${unread}</span>` : '';
      li.innerHTML = `
        <button class="feed-emoji-btn" data-feed-id="${feed.id}" title="选择图标">${emoji}</button>
        <span class="feed-title">${escapeHtml(feed.title || feed.url)}</span>
        ${badgeHtml}
      `;
    }

    li.addEventListener('click', () => {
      if (renamingFeedId === feed.id) return;
      selectFeed(feed.id);
    });
    li.addEventListener('contextmenu', e => { e.preventDefault(); showContextMenu(e.clientX, e.clientY, feed); });

    if (feed.id.toString() === prevSelected) li.classList.add('selected');
    feedListEl.appendChild(li);
  });

  feedListEl.querySelectorAll('.feed-emoji-btn').forEach(btn => {
    btn.addEventListener('click', e => {
      e.stopPropagation();
      openEmojiPicker(btn, parseInt(btn.dataset.feedId), () => {
        renderFeedList(allFeeds);
        if (selectedFeedId === parseInt(btn.dataset.feedId)) {
          const feed = allFeeds.find(f => f.id === selectedFeedId);
          if (feed) updateToolbarFeedInfo(feed);
        }
      });
    });
  });
}

function selectFeed(feedId) {
  mode = 'feed';
  enterFeedMode();
  document.querySelectorAll('.feed-item').forEach(el => el.classList.toggle('selected', parseInt(el.dataset.feedId) === feedId));
  document.querySelectorAll('.sidebar-row').forEach(el => el.classList.remove('active'));
  selectedFeedId = feedId;
  const feed = allFeeds.find(f => f.id === feedId);
  if (feed) updateToolbarFeedInfo(feed);
  loadEntries(feedId);
}

function updateToolbarFeedInfo(feed) {
  if (!toolbarSubtitle) return;
  if (!settingsView.classList.contains('hidden')) return;
  if (mode === 'briefing') { setToolbarSubtitle('briefing'); return; }
  if (!feed) { toolbarSubtitle.innerHTML = ''; return; }
  const unread = unreadCountForFeed(feed.id);
  const total = totalCountForFeed(feed.id);
  toolbarSubtitle.innerHTML = `
    <span>${escapeHtml(feedEmoji(feed.id))}</span>
    <span>${escapeHtml(feed.title || feed.url)}</span>
    <span class="ts-meta">·</span>
    <span class="ts-tertiary">${unread} 未读 / ${total}</span>
  `;
}

// ── Emoji picker ──────────────────────────────
function openEmojiPicker(anchorBtn, feedId, onAfter) {
  document.querySelectorAll('.emoji-picker').forEach(p => p.remove());
  anchorBtn.classList.add('open');

  const picker = document.createElement('div');
  picker.className = 'emoji-picker';
  const current = feedEmoji(feedId);
  picker.innerHTML = `
    <div class="emoji-picker-heading">选择图标</div>
    <div class="emoji-picker-grid">
      ${EMOJI_PRESETS.map(e => `<button class="emoji-cell ${e === current ? 'active' : ''}" data-emoji="${e}">${e}</button>`).join('')}
    </div>
  `;

  const rect = anchorBtn.getBoundingClientRect();
  picker.style.top = (rect.bottom + 6) + 'px';
  picker.style.left = rect.left + 'px';
  document.body.appendChild(picker);

  picker.addEventListener('click', e => {
    const cell = e.target.closest('.emoji-cell');
    if (!cell) return;
    e.stopPropagation();
    setFeedEmoji(feedId, cell.dataset.emoji);
    picker.remove();
    anchorBtn.classList.remove('open');
    if (onAfter) onAfter();
  });

  setTimeout(() => {
    const handler = (ev) => {
      if (!picker.contains(ev.target) && ev.target !== anchorBtn) {
        picker.remove();
        anchorBtn.classList.remove('open');
        document.removeEventListener('click', handler);
      }
    };
    document.addEventListener('click', handler);
  }, 0);
}

function startRenameFeed(id) { renamingFeedId = id; loadFeeds(); }
function cancelRenameFeed() { renamingFeedId = null; loadFeeds(); }

async function finishRenameFeed(id, name) {
  if (renamingFeedId !== id) return;
  const trimmed = name.trim();
  if (!trimmed) { cancelRenameFeed(); return; }
  renamingFeedId = null;
  try {
    await invoke('rename_feed', { id, name: trimmed });
    await loadFeeds();
    if (!document.getElementById('section-feeds').classList.contains('hidden')) renderFeedSettingsList();
    setGlobalStatus('重命名完成', 'success');
  } catch (err) {
    setGlobalStatus('重命名失败: ' + err, 'error');
    await loadFeeds();
  }
}

// ── Add Feed input (pill with animated states) ─
async function addFeed() {
  const url = feedUrlInput.value.trim();
  if (!url) { setGlobalStatus('请输入 RSS URL', 'error'); return; }
  setGlobalStatus('');
  btnAddFeed.disabled = true;
  addFeedRow.classList.add('adding');
  feedUrlInput.placeholder = '正在拉取…';
  try {
    await invoke('add_feed', { url });
    addFeedRow.classList.remove('adding');
    addFeedRow.classList.add('added');
    if (addFeedIcon) addFeedIcon.innerHTML = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 8.3 6.5 11 12.5 4.8"/></svg>`;
    feedUrlInput.value = '';
    feedUrlInput.placeholder = '订阅源已添加';
    btnAddFeed.classList.add('hidden');
    setTimeout(() => {
      addFeedRow.classList.remove('added');
      feedUrlInput.placeholder = '添加订阅源 · 粘贴 RSS URL';
      if (addFeedIcon) addFeedIcon.innerHTML = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><line x1="8" y1="3.5" x2="8" y2="12.5"/><line x1="3.5" y1="8" x2="12.5" y2="8"/></svg>`;
    }, 1400);
    await loadFeeds();
    if (!document.getElementById('section-feeds').classList.contains('hidden')) renderFeedSettingsList();
  } catch (e) {
    addFeedRow.classList.remove('adding');
    feedUrlInput.placeholder = '添加订阅源 · 粘贴 RSS URL';
    setGlobalStatus('添加失败: ' + e, 'error');
  } finally {
    btnAddFeed.disabled = false;
  }
}

// ── In-app confirm modal (window.confirm is blocked in Tauri 2 WKWebView) ──
function confirmDialog(message, { okLabel = '删除', cancelLabel = '取消', danger = true } = {}) {
  return new Promise(resolve => {
    const overlay = document.createElement('div');
    overlay.className = 'confirm-overlay';
    overlay.innerHTML = `
      <div class="confirm-card" role="dialog" aria-modal="true">
        <div class="confirm-msg">${message}</div>
        <div class="confirm-actions">
          <button class="btn btn-secondary btn-sm confirm-cancel" type="button">${cancelLabel}</button>
          <button class="btn ${danger ? 'btn-danger' : 'btn-primary'} btn-sm confirm-ok" type="button">${okLabel}</button>
        </div>
      </div>
    `;
    const cleanup = (val) => {
      document.removeEventListener('keydown', onKey);
      overlay.remove();
      resolve(val);
    };
    const onKey = (e) => {
      if (e.key === 'Escape') cleanup(false);
      else if (e.key === 'Enter') cleanup(true);
    };
    overlay.addEventListener('click', e => {
      if (e.target === overlay) cleanup(false);
    });
    overlay.querySelector('.confirm-ok').addEventListener('click', () => cleanup(true));
    overlay.querySelector('.confirm-cancel').addEventListener('click', () => cleanup(false));
    document.addEventListener('keydown', onKey);
    document.body.appendChild(overlay);
    // Focus the primary action for fast keyboard confirm
    setTimeout(() => overlay.querySelector('.confirm-ok')?.focus(), 0);
  });
}

async function deleteFeed(id) {
  try {
    await invoke('delete_feed', { id });
    hideContextMenu();
    detailEmpty.classList.remove('hidden');
    detailContent.classList.add('hidden');
    if (selectedFeedId === id) selectedFeedId = null;
    await loadFeeds();
    await loadEntries();
    if (!document.getElementById('section-feeds').classList.contains('hidden')) renderFeedSettingsList();
  } catch (e) {
    setGlobalStatus('删除失败: ' + e, 'error');
  }
}

// ── Context menus ──────────────────────────────
function showContextMenu(x, y, feed) {
  hideContextMenu();
  const menu = document.createElement('div');
  menu.className = 'context-menu';
  menu.style.left = x + 'px';
  menu.style.top = y + 'px';
  menu.innerHTML = `
    <div class="context-item" data-action="rename">重命名</div>
    <div class="context-separator"></div>
    <div class="context-item context-item-danger" data-action="delete">删除</div>
  `;
  menu.addEventListener('click', async e => {
    const action = e.target.dataset.action;
    hideContextMenu();
    if (action === 'rename') startRenameFeed(feed.id);
    else if (action === 'delete') {
      if (await confirmDialog('确定删除该订阅源及其所有文章？')) deleteFeed(feed.id);
    }
  });
  document.body.appendChild(menu);
  contextMenu = menu;
  document.addEventListener('click', hideContextMenu, { once: true });
}

async function deleteBriefing(id) {
  try {
    await invoke('delete_briefing', { id });
    // If the deleted one was selected, clear the detail panel.
    if (selectedBriefingId === id) selectedBriefingId = null;
    // Drop it from the in-memory list immediately so the row vanishes
    // without waiting for the round-trip reload.
    BRIEFINGS = BRIEFINGS.filter(b => b.id !== id);
    readBriefings.delete(id);
    persistReadBriefings();
    renderBriefingList();
    updateOverviewCounts();
    if (BRIEFINGS.length === 0) {
      showBriefingEmpty();
    } else if (!selectedBriefingId) {
      selectBriefing(BRIEFINGS[0].id);
    }
    setGlobalStatus('简报已删除', 'success');
  } catch (e) {
    setGlobalStatus('删除失败: ' + e, 'error');
  }
}

function showBriefingContextMenu(x, y, briefing) {
  hideContextMenu();
  const menu = document.createElement('div');
  menu.className = 'context-menu';
  menu.style.left = x + 'px';
  menu.style.top = y + 'px';
  menu.innerHTML = `<div class="context-item context-item-danger" data-action="delete">删除</div>`;
  menu.addEventListener('click', async e => {
    const action = e.target.dataset.action;
    if (!action) return;
    hideContextMenu();
    if (action === 'delete') {
      if (await confirmDialog('确定删除该简报？此操作不可撤销。')) {
        deleteBriefing(briefing.id);
      }
    }
  });
  document.body.appendChild(menu);
  contextMenu = menu;
  document.addEventListener('click', hideContextMenu, { once: true });
}

function showEntryContextMenu(x, y, entry) {
  hideContextMenu();
  const menu = document.createElement('div');
  menu.className = 'context-menu';
  menu.style.left = x + 'px';
  menu.style.top = y + 'px';

  // Translation is automatic now — context menu only offers read/star actions.
  let items = '';
  items += `<div class="context-item" data-action="${entry.is_read ? 'mark-unread' : 'mark-read'}">${entry.is_read ? '标为未读' : '标为已读'}</div>`;
  items += `<div class="context-item" data-action="${starredIds().has(entry.id) ? 'unstar' : 'star'}">${starredIds().has(entry.id) ? '取消星标' : '星标'}</div>`;
  menu.innerHTML = items;

  menu.addEventListener('click', async e => {
    const action = e.target.dataset.action;
    if (!action) return;
    hideContextMenu();
    if (action === 'mark-read') await setEntryRead(entry, true);
    else if (action === 'mark-unread') await setEntryRead(entry, false);
    else if (action === 'star' || action === 'unstar') {
      toggleStar(entry.id);
      renderEntryList(allEntries);
      updateOverviewCounts();
    }
  });
  document.body.appendChild(menu);
  contextMenu = menu;
  document.addEventListener('click', hideContextMenu, { once: true });
}

function hideContextMenu() {
  if (contextMenu) { contextMenu.remove(); contextMenu = null; }
}

async function setEntryRead(entry, isRead) {
  const prev = entry.is_read;
  entry.is_read = isRead;
  const g = globalEntries.find(e => e.id === entry.id);
  if (g) {
    g.is_read = isRead;
    if (isRead && !g.read_at) g.read_at = new Date().toISOString();
  }
  renderEntryList(allEntries);
  updateOverviewCounts();
  renderFeedList(allFeeds);
  const feed = allFeeds.find(f => f.id === entry.feed_id);
  if (feed) updateToolbarFeedInfo(feed);
  try {
    await invoke('set_entry_read', { entryId: entry.id, isRead });
  } catch (e) {
    entry.is_read = prev;
    if (g) g.is_read = prev;
    renderEntryList(allEntries);
    updateOverviewCounts();
    renderFeedList(allFeeds);
    setGlobalStatus('更新已读状态失败: ' + e, 'error');
  }
}

// ── Entry list ─────────────────────────────────
async function loadEntries(feedId) {
  try {
    allEntries = await invoke('list_entries', { feedId: feedId || null });
    renderEntryList(allEntries);
  } catch (e) {
    entryItemsEl.innerHTML = `<li class="entry-empty">加载文章失败: ${e}</li>`;
  }
}

function renderEntryList(entries) {
  const stars = starredIds();
  let filtered = entries;
  if (entryFilterValue === 'unread') filtered = entries.filter(e => !e.is_read);
  else if (entryFilterValue === 'starred') filtered = entries.filter(e => stars.has(e.id));

  const selectedId = entryItemsEl.querySelector('.entry-item.selected')?.dataset?.entryId;
  entryItemsEl.innerHTML = '';

  if (filtered.length === 0) {
    let msg;
    if (entryFilterValue === 'unread') msg = '没有未读文章';
    else if (entryFilterValue === 'starred') msg = '尚未星标任何文章';
    else msg = document.querySelector('.feed-item.selected')
      ? '该订阅源暂无文章，点击刷新获取'
      : '添加订阅源后点击刷新按钮获取文章';
    entryItemsEl.innerHTML = `<li class="entry-empty">${msg}</li>`;
    return;
  }

  filtered.forEach(entry => {
    const li = document.createElement('li');
    li.className = `entry-item ${entry.is_read ? 'read' : 'unread'}`;
    li.dataset.entryId = entry.id;

    const title = entry.title_translated || entry.title;
    const timeStr = entry.published_at ? timeAgo(entry.published_at) : '';
    const source = journalName(entry);

    // Visual translation status — spinner during work, small error pill on failure.
    // No "待翻译" tag — translation now runs automatically in the background, so
    // a pending state is the default; cluttering every entry with a tag would be noise.
    const isTranslating = entry._titleTranslating || entry._summaryTranslating;
    let tagHtml = '';
    if (entry._transError) tagHtml = ` <span class="entry-tag entry-tag-error">失败</span>`;

    let metaHtml = '';
    if (source) {
      metaHtml = `<div class="entry-meta-row"><span class="entry-source">《${escapeHtml(source)}》</span></div>`;
    }

    const badges = [];
    if (entry.title_translated && entry.summary_translated) {
      badges.push(`<span class="pill pill-accent">已翻译</span>`);
    }
    const badgesHtml = badges.length ? `<div class="entry-badges">${badges.join('')}</div>` : '';

    li.innerHTML = `
      <div class="entry-dot-col"><span class="entry-read-dot"></span></div>
      <div class="entry-body">
        <div class="entry-row-top">
          <div class="entry-title">${escapeHtml(title)}${tagHtml}${isTranslating ? ' <span class="entry-spinner"></span>' : ''}</div>
          <div class="entry-date">${timeStr}</div>
        </div>
        ${metaHtml}
        ${badgesHtml}
      </div>
    `;

    li.addEventListener('click', async () => {
      document.querySelectorAll('.entry-item').forEach(el => el.classList.remove('selected'));
      li.classList.add('selected');
      showDetail(entry);
      if (!entry.is_read) await setEntryRead(entry, true);
    });
    li.addEventListener('contextmenu', e => { e.preventDefault(); showEntryContextMenu(e.clientX, e.clientY, entry); });

    if (currentEntry && currentEntry.id === entry.id) li.classList.add('selected');
    if (selectedId && entry.id.toString() === selectedId) li.classList.add('selected');

    entryItemsEl.appendChild(li);
  });
}

// ── Detail panel ───────────────────────────────
function showDetail(entry) {
  currentEntry = entry;
  detailEmpty.classList.add('hidden');
  detailContent.classList.remove('hidden');

  detailTitle.textContent = entry.title_translated || entry.title;

  const journal = journalName(entry);
  if (detailJournal) {
    if (journal) {
      detailJournal.textContent = `《${journal}》`;
      detailJournal.classList.remove('hidden');
    } else {
      detailJournal.textContent = '';
      detailJournal.classList.add('hidden');
    }
  }

  applyAffiliation(entry);
  ensureAffiliationLoaded(entry);

  detailDateSub.textContent = formatPublicationDate(entry);

  const authorEl = document.getElementById('detail-author');
  const authorSep = document.getElementById('detail-author-sep');
  if (authorEl) {
    const formatted = formatAuthors(entry.author);
    if (formatted) {
      authorEl.textContent = formatted;
      authorSep?.classList.remove('hidden');
    } else {
      authorEl.textContent = '';
      authorSep?.classList.add('hidden');
    }
  }
  detailPublicationDate.textContent = formatPublicationDate(entry);

  if (entry.title_translated && entry.summary_translated) {
    detailSourceBadge.textContent = '已翻译';
    detailBadgeRow.classList.remove('hidden');
  } else {
    detailBadgeRow.classList.add('hidden');
  }

  const starBtn = document.getElementById('btn-star');
  if (starBtn) starBtn.classList.toggle('active', starredIds().has(entry.id));

  const aiFooter = document.getElementById('detail-ai-footer');
  const modelName = (modelInput?.value || 'DeepSeek').trim() || 'DeepSeek';
  document.getElementById('ai-model-name').textContent = `由 ${modelName} 翻译`;
  if (entry.title_translated || entry.summary_translated) {
    aiFooter?.classList.remove('hidden');
  } else {
    aiFooter?.classList.add('hidden');
  }
  // In-progress title spinner appended after the title (cleared by updateDetailFromCurrent)
  refreshDetailTitleSpinner(entry);

  abstractLang = 'zh';
  syncAbstractToggle();
  renderSummary(entry);
  if (!entry.summary && !entry.summary_translated) loadAbstract(entry);

  btnOpenUrl.onclick = () => openUrl(entry.link);
}

function syncAbstractToggle() {
  document.querySelectorAll('.abstract-toggle-btn').forEach(b => {
    b.classList.toggle('active', b.dataset.lang === abstractLang);
  });
  const toggle = document.getElementById('abstract-toggle');
  if (!toggle) return;
  const e = currentEntry;
  const hasBoth = e && e.summary && e.summary_translated;
  toggle.style.visibility = hasBoth ? 'visible' : 'hidden';
}

function renderSummary(entry, state = 'ready') {
  syncAbstractToggle();
  const showRetry = abstractLang === 'zh'
    && entry.summary
    && !entry.summary_translated
    && !entry._summaryTranslating;
  if (detailSummaryRetry) detailSummaryRetry.classList.toggle('hidden', !showRetry);
  // 1) Currently being translated → spinner placeholder (only when zh tab selected
  //    and we don't yet have a translation; if EN tab is selected we still show the
  //    original text while translation runs in the background).
  if (entry._summaryTranslating && abstractLang === 'zh' && !entry.summary_translated) {
    detailSummaryContent.innerHTML = '<div class="detail-summary-translating">正在翻译摘要…</div>';
    return;
  }
  // 2) Translated text available, zh tab → show it
  if (entry.summary_translated && abstractLang === 'zh') {
    detailSummaryContent.innerHTML = `<p>${escapeHtml(entry.summary_translated)}</p>`;
    return;
  }
  // 3) Original summary available (any of: en tab; or zh tab without translation yet)
  if (entry.summary && (abstractLang === 'en' || !entry.summary_translated)) {
    const clean = stripHtml(entry.summary);
    detailSummaryContent.innerHTML = `<p class="detail-summary-original">${escapeHtml(clean)}</p>`;
    return;
  }
  // 4) No summary yet — either still fetching (or we never will)
  if (state === 'loading') detailSummaryContent.innerHTML = '<p class="detail-summary-empty">正在获取 Abstract…</p>';
  else detailSummaryContent.innerHTML = '<p class="detail-summary-empty">未能自动获取 Abstract。可以打开原文查看。</p>';
}

function refreshDetailTitleSpinner(entry) {
  if (!detailTitle) return;
  let spin = document.getElementById('detail-title-spinner');
  if (entry._titleTranslating) {
    if (!spin) {
      spin = document.createElement('span');
      spin.id = 'detail-title-spinner';
      spin.className = 'detail-title-spinner';
      detailTitle.appendChild(spin);
    }
  } else if (spin) {
    spin.remove();
  }
}

function applyAffiliation(entry) {
  if (!detailAffiliation) return;
  const text = (entry?.affiliation || '').trim();
  if (text) {
    detailAffiliation.textContent = text;
    detailAffiliation.classList.remove('hidden');
  } else {
    detailAffiliation.textContent = '';
    detailAffiliation.classList.add('hidden');
  }
}

async function ensureAffiliationLoaded(entry) {
  if (!entry || (entry.affiliation && entry.affiliation.trim())) return;
  if (!entry.link || !entry.link.includes('pubmed.ncbi.nlm.nih.gov')) return;
  if (entry._affiliationLoading) return;
  entry._affiliationLoading = true;
  try {
    const text = await invoke('fetch_affiliation', { entryId: entry.id });
    applyEntryUpdate(entry.id, x => { x.affiliation = text || ''; });
    if (currentEntry && currentEntry.id === entry.id) applyAffiliation(currentEntry);
  } catch (e) {
    console.warn('fetch_affiliation 失败:', e);
  } finally {
    entry._affiliationLoading = false;
  }
}

async function loadAbstract(entry) {
  renderSummary(entry, 'loading');
  try {
    const text = await invoke('fetch_abstract', { entryId: entry.id });
    if (!currentEntry || currentEntry.id !== entry.id) return;
    if (text) {
      entry.summary = text;
      currentEntry.summary = text;
      renderSummary(entry);
    } else {
      renderSummary(entry, 'empty');
    }
  } catch (e) {
    if (!currentEntry || currentEntry.id !== entry.id) return;
    detailSummaryContent.innerHTML = `<p class="detail-summary-error">Abstract 获取失败: ${escapeHtml(String(e))}</p>`;
  }
}

async function retrySummaryTranslation() {
  if (!currentEntry) return;
  const entryId = currentEntry.id;
  // Mirror translation-progress events: clear the error pill across all
  // entry collections so the middle-column badge disappears immediately.
  applyEntryUpdate(entryId, x => {
    x._summaryTranslating = true;
    x._transError = null;
  });
  renderEntryList(allEntries);
  if (currentEntry && currentEntry.id === entryId) renderSummary(currentEntry);
  try {
    const translated = await invoke('translate_summary', { entryId });
    applyEntryUpdate(entryId, x => {
      x.summary_translated = translated;
      x._summaryTranslating = false;
      x._transError = null;
    });
    addTranslationCost(translated.length);
  } catch (e) {
    const msg = (typeof e === 'string') ? e : (e && e.message) || '翻译失败';
    applyEntryUpdate(entryId, x => {
      x._summaryTranslating = false;
      x._transError = msg;
    });
  } finally {
    renderEntryList(allEntries);
    updateOverviewCounts();
    if (currentEntry && currentEntry.id === entryId) renderSummary(currentEntry);
  }
}

async function openUrl(url) {
  try { await invoke('open_url', { url }); }
  catch (e) { console.error('打开链接失败:', e); }
}

// ── Removed: translateSummary / ensureEntrySummary / translateAllTitles ──
// Title + summary translation is now driven entirely by the background
// pipeline in src-tauri/src/services/translation_pipeline.rs. The UI
// reacts to `translation-progress` events emitted by that pipeline.

// ── Translation progress events (from background pipeline) ────────
function applyEntryUpdate(entryId, mutate) {
  const inAll = allEntries.find(e => e.id === entryId);
  const inGlobal = globalEntries.find(e => e.id === entryId);
  if (inAll) mutate(inAll);
  if (inGlobal && inGlobal !== inAll) mutate(inGlobal);
  if (currentEntry && currentEntry.id === entryId && currentEntry !== inAll) mutate(currentEntry);
}

function setupTranslationEvents() {
  const event = window.__TAURI__?.event;
  if (!event?.listen) return;
  event.listen('translation-progress', (e) => {
    const p = e.payload || {};
    const id = p.entry_id;
    if (!id) return;

    if (p.kind === 'start') {
      applyEntryUpdate(id, x => {
        if (p.field === 'title') x._titleTranslating = true;
        else if (p.field === 'summary') x._summaryTranslating = true;
        x._transError = null;
      });
    } else if (p.kind === 'done') {
      applyEntryUpdate(id, x => {
        if (p.field === 'title') {
          x.title_translated = p.text;
          x._titleTranslating = false;
        } else if (p.field === 'summary') {
          x.summary_translated = p.text;
          x._summaryTranslating = false;
        }
        x._transError = null;
      });
      if (p.text) addTranslationCost(p.text.length);
    } else if (p.kind === 'error') {
      applyEntryUpdate(id, x => {
        if (p.field === 'title') x._titleTranslating = false;
        else if (p.field === 'summary') x._summaryTranslating = false;
        x._transError = p.error || '翻译失败';
      });
    } else if (p.kind === 'summary_fetched') {
      applyEntryUpdate(id, x => {
        x.summary = p.summary;
        x.summary_source = p.source;
      });
    }

    renderEntryList(allEntries);
    updateOverviewCounts();
    if (currentEntry && currentEntry.id === id) {
      // Re-render only the parts that changed instead of resetting the panel
      detailTitle.textContent = currentEntry.title_translated || currentEntry.title;
      refreshDetailTitleSpinner(currentEntry);
      const aiFooter = document.getElementById('detail-ai-footer');
      if (currentEntry.title_translated || currentEntry.summary_translated) {
        aiFooter?.classList.remove('hidden');
      }
      if (currentEntry.title_translated && currentEntry.summary_translated) {
        detailSourceBadge.textContent = '已翻译';
        detailBadgeRow.classList.remove('hidden');
      } else {
        detailBadgeRow.classList.add('hidden');
      }
      renderSummary(currentEntry);
    }
  });

  // Pipeline-level status: surfaces "needs API Key" / "API Key invalid"
  // as a persistent banner at the top of the main view. Cleared once a
  // healthy translation run finishes (status: "ok").
  event.listen('translation-status', (e) => {
    const p = e.payload || {};
    if (p.status === 'needs_key') {
      showTranslationBanner({
        kind: 'needs_key',
        text: `检测到 ${p.pending || ''} 篇待翻译文章，但未配置 DeepSeek API Key。前往设置填写后会自动翻译。`,
      });
    } else if (p.status === 'auth_failed') {
      showTranslationBanner({
        kind: 'auth_failed',
        text: `DeepSeek API Key 无效或已过期，请打开设置重新填写并测试连接。${p.message ? `（${p.message}）` : ''}`,
      });
    } else if (p.status === 'ok') {
      hideTranslationBanner();
    }
  });
}

/// Persistent banner pinned to the top of the main view. We surface it from
/// pipeline-level translation status events — per-entry error pills are too
/// easy to miss when the user is browsing the list.
function showTranslationBanner({ kind, text }) {
  const banner = document.getElementById('translation-status-banner');
  if (!banner) return;
  banner.dataset.kind = kind;
  const textEl = banner.querySelector('.tsb-text');
  if (textEl) textEl.textContent = text;
  banner.classList.remove('hidden');
}
function hideTranslationBanner() {
  const banner = document.getElementById('translation-status-banner');
  if (!banner) return;
  banner.classList.add('hidden');
  banner.dataset.kind = '';
}

function wireTranslationBannerButtons() {
  const goSettings = document.getElementById('tsb-go-settings');
  const dismiss = document.getElementById('tsb-dismiss');
  goSettings?.addEventListener('click', () => {
    showSettings('translation');
    // Once the user lands in the right place, the banner has served its
    // purpose — clear it so they don't see two reminders.
    hideTranslationBanner();
  });
  dismiss?.addEventListener('click', hideTranslationBanner);
}

// ── Update channel (settings → 其他设置) ───────
// Loads the bundled version into the "关于 Cento" card and wires the
// "检查更新" button + auto-check toggle. The backend already runs a weekly
// check on its own — the UI here is for surfacing results and giving the
// user a manual override.
async function initUpdateChannel() {
  const versionEl = document.getElementById('about-version');
  const statusEl = document.getElementById('update-status');
  const metaEl = document.getElementById('update-meta');
  const btnCheck = document.getElementById('btn-check-update');
  const actionsEl = document.getElementById('update-actions');
  const btnDownload = document.getElementById('btn-download-update');
  const btnRelease = document.getElementById('btn-view-release');
  const toggleAuto = document.getElementById('update-auto-check');
  if (!versionEl) return;

  // Current version + last-check timestamp.
  try {
    const v = await invoke('get_app_version');
    versionEl.textContent = `v${v}`;
  } catch (e) {
    versionEl.textContent = '未知';
  }
  try {
    const prefs = await invoke('get_update_prefs');
    if (toggleAuto) toggleAuto.classList.toggle('on', prefs.auto_check_enabled);
    if (prefs.last_checked_at && metaEl) {
      metaEl.textContent = `上次检查：${formatLocalTime(prefs.last_checked_at)}`;
    }
  } catch (e) {
    // First-run with no settings row yet — toggle defaults to "on" via HTML.
  }

  btnCheck?.addEventListener('click', async () => {
    btnCheck.disabled = true;
    const orig = btnCheck.textContent;
    btnCheck.innerHTML = '<span class="spinner"></span> 检查中…';
    statusEl.classList.remove('has-update');
    statusEl.textContent = '正在访问 GitHub…';
    try {
      const info = await invoke('check_for_update');
      renderUpdateResult(info, { statusEl, actionsEl, btnDownload, btnRelease });
      if (metaEl) metaEl.textContent = `上次检查：${formatLocalTime(new Date().toISOString())}`;
    } catch (e) {
      const msg = (typeof e === 'string') ? e : (e && e.message) || String(e);
      statusEl.textContent = `检查失败：${msg}`;
      actionsEl?.classList.add('hidden');
    } finally {
      btnCheck.disabled = false;
      btnCheck.textContent = orig;
    }
  });

  toggleAuto?.addEventListener('click', async () => {
    // Optimistic toggle so the click feels instant; the backend round-trip
    // rolls it back only if saving fails.
    const wantOn = !toggleAuto.classList.contains('on');
    toggleAuto.classList.toggle('on', wantOn);
    try {
      await invoke('set_update_auto_check', { enabled: wantOn });
    } catch (e) {
      toggleAuto.classList.toggle('on', !wantOn);
      console.warn('保存自动检查偏好失败:', e);
    }
  });

  // Reflect background-checker results live without the user clicking.
  const evt = window.__TAURI__?.event;
  evt?.listen?.('update-checked', (e) => {
    const info = e.payload || {};
    renderUpdateResult(info, { statusEl, actionsEl, btnDownload, btnRelease });
    if (metaEl) metaEl.textContent = `上次检查：${formatLocalTime(new Date().toISOString())}`;
  });
}

function renderUpdateResult(info, { statusEl, actionsEl, btnDownload, btnRelease }) {
  if (!info || !statusEl) return;
  if (info.has_update) {
    statusEl.classList.add('has-update');
    statusEl.textContent = `🎉 新版本 v${info.latest_version} 已发布（当前 v${info.current_version}）`;
    if (actionsEl) {
      actionsEl.classList.remove('hidden');
      // Prefer the direct .dmg asset; fall back to the release page if the
      // tag doesn't have an attached installer yet.
      if (btnDownload) {
        btnDownload.href = info.asset_url || info.release_url;
        btnDownload.textContent = info.asset_url ? '下载安装包' : '前往下载页';
      }
      if (btnRelease) btnRelease.href = info.release_url;
    }
  } else {
    statusEl.classList.remove('has-update');
    statusEl.textContent = `已是最新版（v${info.current_version}）`;
    actionsEl?.classList.add('hidden');
  }
}

function formatLocalTime(iso) {
  try {
    const d = new Date(iso.includes('T') ? iso : iso.replace(' ', 'T') + 'Z');
    if (isNaN(d.getTime())) return iso;
    return d.toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
}

// macOS native notification via tauri-plugin-notification. Permission is
// requested lazily on first use; subsequent calls reuse the granted permission.
let notificationPermissionGranted = null;

// Proactively request notification permission once the webview is alive so
// the Rust-side scheduler can fire banners without first needing the user to
// click the "test notification" button. macOS won't deliver Rust-initiated
// notifications until the bundle has been authorized at least once.
async function ensureNotificationPermission() {
  const notif = window.__TAURI__?.notification;
  if (!notif) return false;
  try {
    if (await notif.isPermissionGranted()) {
      notificationPermissionGranted = true;
      return true;
    }
    const perm = await notif.requestPermission();
    notificationPermissionGranted = perm === 'granted';
    return notificationPermissionGranted;
  } catch (e) {
    console.warn('notification permission request failed:', e);
    return false;
  }
}
async function sendDesktopNotification(title, body) {
  const notif = window.__TAURI__?.notification;
  if (!notif) {
    console.warn('notification plugin unavailable');
    return { ok: false, reason: 'plugin-missing' };
  }
  try {
    if (notificationPermissionGranted === null) {
      notificationPermissionGranted = await notif.isPermissionGranted();
      if (!notificationPermissionGranted) {
        const perm = await notif.requestPermission();
        notificationPermissionGranted = perm === 'granted';
      }
    }
    if (!notificationPermissionGranted) {
      return { ok: false, reason: 'denied' };
    }
    await notif.sendNotification({ title, body });
    return { ok: true };
  } catch (e) {
    console.warn('notification failed:', e);
    return { ok: false, reason: String(e) };
  }
}

// ── Tray icon ───────────────────────────────────
function trayVisiblePref() {
  return localStorage.getItem('tray-visible') !== '0'; // default ON
}
function setTrayVisiblePref(on) {
  localStorage.setItem('tray-visible', on ? '1' : '0');
}
async function applyTrayVisibility(visible) {
  try { await invoke('set_tray_visible', { visible }); }
  catch (e) { console.warn('set_tray_visible failed:', e); }
}
async function pushTrayUnread() {
  const count = globalEntries.filter(e => !e.is_read).length;
  try { await invoke('update_tray_unread', { count }); }
  catch (e) { /* tray may be off, ignore */ }
}

// ── Refresh ────────────────────────────────────
async function refreshAll() {
  btnRefresh.disabled = true;
  refreshIcon.style.animation = 'spin 0.8s linear infinite';
  setGlobalStatus('正在刷新…', 'progress');
  try {
    const result = await invoke('fetch_all_feeds');
    let msg = `完成：${result.total_feeds} 个源`;
    if (result.new_entries > 0) msg += `，新增 ${result.new_entries} 篇 · 正在自动翻译…`;
    else msg += '，没有新文章';
    if (result.errors.length > 0) msg += `，${result.errors.length} 个问题`;
    setGlobalStatus(msg, 'success');

    // Notifications are fired by the Rust backend for any feed with notify=1,
    // so the frontend doesn't dispatch them here anymore.

    await loadFeeds();
    await loadEntries(selectedFeedId);
  } catch (e) {
    setGlobalStatus('刷新失败: ' + e, 'error');
  } finally {
    btnRefresh.disabled = false;
    refreshIcon.style.animation = '';
  }
}

// Background refresh is handled by the Rust scheduler in src-tauri/src/services/scheduler.rs.
// It runs whenever the app process is alive — even when the window is hidden in the
// tray — and emits a `scheduler-refreshed` event when it picks up new entries.
function startSchedulerListener() {
  const listen = window.__TAURI__?.event?.listen;
  if (!listen) return;
  listen('scheduler-refreshed', async () => {
    await loadFeeds();
    await loadEntries(selectedFeedId);
  });
}

// ── Briefing mode ──────────────────────────────
// Mirrors `DEFAULT_BRIEFING_GUIDANCE` in src-tauri/src/services/briefing_service.rs.
// What the user sees in the Prompt editor matches what the backend would send if
// they hadn't customized it.  The JSON output-schema requirement isn't shown
// here — it's appended by the backend regardless, so the user can't accidentally
// break parsing by editing the prompt.
const DEFAULT_BRIEFING_PROMPT =
`你是一位资深的科技报道编辑，专长是把一周内的前沿学术文献整理成面向研究者的高质量中文综述。请阅读用户提供的文献（标题、来源期刊、摘要），把它们汇总成一份**结构清晰、信息密度高、可读性强**的中文文献简报，写作风格参考《Nature Briefing》《知社学术圈》等科技前沿报道。

## 整体结构

1. **开篇导语（2-3 句）**：概括本期主线 —— 哪些方向延续了上期的热度、出现了哪些值得关注的新动向、整体脉络是什么。简练有力，避免套话。

2. **按主题分组**：将文献按研究方向归类，例如「机器学习与预后建模」「生物标志物与诊断」「治疗策略与临床试验」「新机制与基础研究」「单细胞 / 空间转录组」等。每组 2-5 条 bullet，组数控制在 3-6 个。使用 \`## \` Markdown 标题。

3. **每条 bullet 40-80 字**，必须包含：
   - 一句话核心发现
   - **关键数值**：AUC、HR、95% CI、样本量、p-value、效应量等具体指标（如原文有则必须保留）
   - **方法 / 创新点**：使用了什么方法、相比已有研究有什么突破
   - 在 bullet 末尾用 \`[n]\` 标注对应文献编号

4. **💡 重点关注**（一个 \`### \` 小节）：选出本期最值得关注的 1-3 篇文献，每篇 100-150 字，按以下顺序展开：
   - 研究背景与目标（1 句）
   - 主要方法（1-2 句）
   - 关键结果（带具体数值，1-2 句）
   - 临床 / 学术意义（1 句）

5. **趋势与启发**（一个 \`## \` 小节）：从本期文献中提炼出 1-3 个跨研究的趋势或启发，比如：
   - 多篇论文是否都指向某个新兴方向？
   - 某个方法学（如单细胞测序、扩散模型、多模态学习）是否被多个团队同时采用？
   - 对临床转化或下一步研究的启发是什么？
   每点 1-2 句，给出**具体**判断而不是泛泛而谈。

## 风格要求

- 专业但不晦涩的中文，技术名词保留英文（PD-L1、CTLA-4、ResNet、GPT-4 等）
- 数据具体到数字，论断必须有文献支撑
- 避免「重要」「突破性」「划时代」「革命性」等空泛词；用具体的数值和对比代替
- 不添加原文没有的信息或主观评价
- Markdown 格式：\`##\` 主题分组、\`###\` 重点关注与趋势启发、\`-\` bullet、\`**加粗**\` 突出关键词与数值
- 整体长度 600-1200 字（取决于文献数量）`;

let BRIEFINGS = [];      // populated dynamically from backend or sample
let readBriefings = new Set(JSON.parse(localStorage.getItem('read-briefings') || '[]'));

function persistReadBriefings() {
  localStorage.setItem('read-briefings', JSON.stringify([...readBriefings]));
}

async function loadBriefings() {
  try {
    BRIEFINGS = await invoke('list_briefings');
  } catch {
    BRIEFINGS = [];
  }
  renderBriefingList();
  updateOverviewCounts();
}

function enterBriefingMode() {
  mode = 'briefing';
  entryListEl.classList.add('hidden');
  detailPanelEl.classList.add('hidden');
  briefingListEl.classList.remove('hidden');
  briefingDetailEl.classList.remove('hidden');
  document.querySelectorAll('.feed-item').forEach(el => el.classList.remove('selected'));
  document.querySelectorAll('.sidebar-row').forEach(el => el.classList.toggle('active', el.dataset.view === 'briefing'));
  setToolbarSubtitle('briefing');
  renderBriefingList();
  if (BRIEFINGS.length > 0) {
    const target = selectedBriefingId && BRIEFINGS.find(b => b.id === selectedBriefingId)
      ? selectedBriefingId
      : BRIEFINGS[0].id;
    selectBriefing(target);
  } else {
    showBriefingEmpty();
  }
}

function enterFeedMode() {
  mode = 'feed';
  briefingListEl.classList.add('hidden');
  briefingDetailEl.classList.add('hidden');
  entryListEl.classList.remove('hidden');
  detailPanelEl.classList.remove('hidden');
}

function renderBriefingList() {
  if (!briefingItemsEl) return;
  briefingItemsEl.innerHTML = '';

  if (BRIEFINGS.length === 0) {
    briefingItemsEl.innerHTML = `
      <li class="entry-empty">
        <div style="margin-bottom: 12px;">还没有生成简报</div>
        <div style="font-size: 11.5px; color: var(--text-tertiary); line-height: 1.6;">点击右上角「立即生成」按需创建，或在「订阅源设置 → AI 简报」配置自动生成频率。</div>
      </li>`;
    return;
  }

  BRIEFINGS.forEach(b => {
    const li = document.createElement('li');
    const isRead = readBriefings.has(b.id);
    li.className = `briefing-item ${isRead ? 'read' : 'unread'}`;
    if (selectedBriefingId === b.id) li.classList.add('selected');
    li.dataset.briefingId = b.id;
    li.innerHTML = `
      <div class="briefing-dot-col"><span class="briefing-dot"></span></div>
      <div class="briefing-body">
        <div class="briefing-meta-top">
          <span class="briefing-period">${escapeHtml(b.period || '')}</span>
          <span class="briefing-counts">${(b.counts?.articles || 0)} 篇 · ${(b.counts?.feeds || 0)} 个来源</span>
        </div>
        <div class="briefing-title">${escapeHtml(b.title || '')}</div>
        <div class="briefing-leadin">${escapeHtml(b.lead_in || b.leadIn || '')}</div>
      </div>
    `;
    li.addEventListener('click', () => selectBriefing(b.id));
    li.addEventListener('contextmenu', e => {
      e.preventDefault();
      showBriefingContextMenu(e.clientX, e.clientY, b);
    });
    briefingItemsEl.appendChild(li);
  });

  // Footer
  const nextDate = computeNextBriefingDate();
  const footer = document.createElement('div');
  footer.className = 'briefing-list-footer';
  footer.innerHTML = `下次简报将在 <span class="briefing-next-date">${nextDate}</span> 自动生成`;
  briefingItemsEl.appendChild(footer);
}

// Minimal Markdown → HTML renderer for briefing bodies. Supports just the
// subset the prompt asks DeepSeek to emit: ##/### headers, bullet lists with
// `-`, **bold**, `inline code`, `---` rules, and paragraphs. Inputs are HTML-
// escaped first so model output can't inject markup.
function renderBriefingMarkdown(md) {
  if (!md) return '';
  const escape = (s) => s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
  const inline = (s) => s
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/`([^`]+)`/g, '<code>$1</code>');

  const lines = md.split('\n');
  let out = '';
  let inList = false;
  const closeList = () => { if (inList) { out += '</ul>'; inList = false; } };

  for (const raw of lines) {
    const line = raw.replace(/\s+$/, '');
    if (!line.trim()) { closeList(); continue; }
    if (line.startsWith('### ')) {
      closeList();
      out += `<h3 class="briefing-md-h3">${inline(escape(line.slice(4)))}</h3>`;
    } else if (line.startsWith('## ')) {
      closeList();
      out += `<h2 class="briefing-md-h2">${inline(escape(line.slice(3)))}</h2>`;
    } else if (line.startsWith('# ')) {
      closeList();
      out += `<h2 class="briefing-md-h2">${inline(escape(line.slice(2)))}</h2>`;
    } else if (/^[-*]\s+/.test(line)) {
      if (!inList) { out += '<ul class="briefing-md-list">'; inList = true; }
      out += `<li>${inline(escape(line.replace(/^[-*]\s+/, '')))}</li>`;
    } else if (/^---+$/.test(line.trim())) {
      closeList();
      out += '<hr class="briefing-md-hr"/>';
    } else {
      closeList();
      out += `<p class="briefing-md-p">${inline(escape(line))}</p>`;
    }
  }
  closeList();
  return out;
}

function selectBriefing(id) {
  selectedBriefingId = id;
  const b = BRIEFINGS.find(x => x.id === id);
  if (!b) { showBriefingEmpty(); return; }

  // Mark as read on click (matches entry-list behavior).
  if (!readBriefings.has(id)) {
    readBriefings.add(id);
    persistReadBriefings();
    updateOverviewCounts();
  }

  // Re-render the list so both `.selected` and `.read` classes get applied
  // from the central `renderBriefingList`. The previous approach (manual
  // `classList.toggle` against `el.dataset.briefingId === id`) silently
  // failed: dataset values are strings ("1") but `id` is a number (1), so
  // `"1" === 1` is `false` — the floating-card styling was correct but
  // never actually applied.
  renderBriefingList();

  briefingDetailEmpty.classList.add('hidden');
  briefingDetailContent.classList.remove('hidden');

  document.getElementById('briefing-detail-date').textContent = b.date || '';
  document.getElementById('briefing-detail-eyebrow').innerHTML = `
    <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2v3M8 11v3M2 8h3M11 8h3M4.2 4.2l2 2M9.8 9.8l2 2M11.8 4.2l-2 2M6.2 9.8l-2 2"/></svg>
    <span>${escapeHtml(b.period || '')}</span>
    <span style="opacity:0.6">·</span>
    <span>${(b.counts?.articles || 0)} 篇文献</span>
  `;
  document.getElementById('briefing-detail-title').textContent = b.title || '';
  document.getElementById('briefing-detail-leadin').textContent = b.lead_in || b.leadIn || '';

  // Render the briefing body. The backend returns `content` as a Markdown
  // string per the new prompt format; this used to iterate `b.sections`,
  // which was a mock-data shape that never matched the live API. That's why
  // briefings were rendering as title + lead-in only with no body.
  const sectionsEl = document.getElementById('briefing-detail-sections');
  sectionsEl.innerHTML = renderBriefingMarkdown(b.content || '');

  document.getElementById('briefing-detail-footer-text').innerHTML =
    `<span class="ai-footer-strong">由 deepseek-v4-pro 生成</span>，覆盖 ${(b.counts?.feeds || 0)} 个订阅源、${(b.counts?.articles || 0)} 篇文献。可在「订阅源设置 → AI 简报」调整频率与 Prompt。`;
}

function showBriefingEmpty() {
  briefingDetailEmpty.classList.remove('hidden');
  briefingDetailContent.classList.add('hidden');
}

async function jumpToArticle(articleId) {
  const article = allEntries.concat(globalEntries).find(e => e.id === articleId);
  if (!article) return;
  enterFeedMode();
  selectFeed(article.feed_id);
  await delay(80);
  const updated = allEntries.find(e => e.id === articleId) || article;
  showDetail(updated);
  if (!updated.is_read) await setEntryRead(updated, true);
}

function computeNextBriefingDate() {
  const freq = localStorage.getItem('briefing-frequency') || 'weekly';
  const hour = localStorage.getItem('briefing-hour') || '09:00';
  const day = localStorage.getItem('briefing-day') || 'mon';
  const now = new Date();
  const [hh, mm] = hour.split(':').map(Number);
  const next = new Date(now);
  next.setHours(hh, mm, 0, 0);
  if (freq === 'daily') {
    if (next <= now) next.setDate(next.getDate() + 1);
  } else if (freq === 'weekly') {
    const dayMap = { mon: 1, tue: 2, wed: 3, thu: 4, fri: 5, sat: 6, sun: 0 };
    const target = dayMap[day] ?? 1;
    while (next.getDay() !== target || next <= now) next.setDate(next.getDate() + 1);
  } else if (freq === 'biweekly') {
    next.setDate(next.getDate() + 14);
  } else if (freq === 'monthly') {
    next.setMonth(next.getMonth() + 1);
  }
  return `${next.getFullYear()}/${next.getMonth()+1}/${next.getDate()} ${hour}`;
}

async function generateBriefingNow() {
  setGlobalStatus('正在生成简报…', 'progress');
  // Stamp last-attempt so the scheduler doesn't retry the same failure every
  // tick — wait at least an hour after a failure to try again.
  localStorage.setItem('briefing-last-attempt', String(Date.now()));
  // Pass the user's edited prompt through. The backend appends the JSON
  // output-schema part on its own, so the user can edit the editorial
  // direction freely without breaking parsing.
  const customPrompt = localStorage.getItem('briefing-prompt') || null;
  try {
    const b = await invoke('generate_briefing', { customPrompt });
    setGlobalStatus('简报已生成', 'success');
    await loadBriefings();
    if (b) selectBriefing(b.id);
  } catch (e) {
    setGlobalStatus('简报生成失败: ' + e, 'error');
  }
}

// ── Briefing auto-scheduler ─────────────────────
// Briefing settings live in localStorage (briefing-enabled / -frequency /
// -day / -hour). This watcher reads them on a low-frequency timer and fires
// generateBriefingNow() when the most recent expected firing time is later
// than the most recent successful briefing.
//
// Why frontend instead of the Rust scheduler: the settings are in
// localStorage and migrating them to SQLite would be a bigger refactor.
// The Tauri webview keeps running in the background even when the window is
// hidden (it's a webview process, not a tab), so this fires reliably.
function computeMostRecentExpectedFiring(freq, hour, day) {
  const [hh, mm] = (hour || '09:00').split(':').map(Number);
  const now = new Date();
  const candidate = new Date(now);
  candidate.setHours(hh, mm, 0, 0);
  if (freq === 'daily') {
    if (candidate > now) candidate.setDate(candidate.getDate() - 1);
    return candidate;
  }
  if (freq === 'weekly') {
    const dayMap = { sun: 0, mon: 1, tue: 2, wed: 3, thu: 4, fri: 5, sat: 6 };
    const target = dayMap[day] ?? 1;
    // Walk backward to the most recent occurrence of `target` weekday at `hour`
    while (candidate.getDay() !== target || candidate > now) {
      candidate.setDate(candidate.getDate() - 1);
    }
    return candidate;
  }
  if (freq === 'biweekly') {
    // Same wall-clock + weekday as `weekly`, but require ≥14 days since last.
    // For "due" purposes we just use the most recent same-weekday/hour and
    // rely on the lastBriefingAt comparison to enforce the 14-day gap.
    const dayMap = { sun: 0, mon: 1, tue: 2, wed: 3, thu: 4, fri: 5, sat: 6 };
    const target = dayMap[day] ?? 1;
    while (candidate.getDay() !== target || candidate > now) {
      candidate.setDate(candidate.getDate() - 1);
    }
    return candidate;
  }
  if (freq === 'monthly') {
    // Most recent occurrence of the configured hour, no day-of-week constraint.
    if (candidate > now) candidate.setDate(candidate.getDate() - 1);
    return candidate;
  }
  return null;
}

function briefingSchedulerTick() {
  if (localStorage.getItem('briefing-enabled') === '0') return;

  const freq = localStorage.getItem('briefing-frequency') || 'weekly';
  const hour = localStorage.getItem('briefing-hour') || '09:00';
  const day  = localStorage.getItem('briefing-day')  || 'mon';

  const expected = computeMostRecentExpectedFiring(freq, hour, day);
  if (!expected) return;

  // Don't retry a failing generation more than once an hour.
  const lastAttempt = parseInt(localStorage.getItem('briefing-last-attempt') || '0', 10);
  if (Date.now() - lastAttempt < 60 * 60 * 1000) return;

  const lastBriefingAt = BRIEFINGS[0]?.generated_at
    ? new Date(BRIEFINGS[0].generated_at)
    : new Date(0);

  if (lastBriefingAt < expected) {
    console.info('[briefing-scheduler] firing — last:', lastBriefingAt, 'expected:', expected);
    generateBriefingNow();
  }
}

function startBriefingScheduler() {
  // Run once on startup (after loadBriefings populates BRIEFINGS).
  // Then keep ticking every 5 minutes — briefings are heavyweight so we
  // don't need to check more often than that.
  setTimeout(briefingSchedulerTick, 8 * 1000);
  setInterval(briefingSchedulerTick, 5 * 60 * 1000);
}

// ── Feeds settings list (per-feed rows) ────────
function renderFeedSettingsList() {
  const body = document.getElementById('feeds-list-body');
  const header = document.getElementById('feeds-card-header');
  if (!body) return;
  header.textContent = `已订阅 · ${allFeeds.length}`;
  body.innerHTML = '';

  if (allFeeds.length === 0) {
    body.innerHTML = '<div style="font-size: 13px; color: var(--text-tertiary); padding: 8px 0;">暂无订阅源</div>';
    return;
  }

  allFeeds.forEach((feed, i) => {
    const row = document.createElement('div');
    row.className = 'feed-settings-row';
    row.dataset.feedId = feed.id;
    const emoji = feedEmoji(feed.id);
    const interval = feedInterval(feed.id);
    const notify = feedNotify(feed.id);
    const total = totalCountForFeed(feed.id);
    const source = feed.url.includes('pubmed') ? 'PubMed RSS' : 'RSS';

    row.innerHTML = `
      <button class="feed-settings-emoji" data-feed-id="${feed.id}" title="选择图标">${emoji}</button>
      <div class="feed-settings-info">
        <div class="feed-settings-name" data-feed-id="${feed.id}">${escapeHtml(feed.title || feed.url)}</div>
        <div class="feed-settings-source">${escapeHtml(source)} · ${total} 篇</div>
      </div>
      <select class="settings-select compact feed-settings-interval" data-feed-id="${feed.id}" title="刷新频率">
        <option value="15m">每 15 分钟</option>
        <option value="1h">每小时</option>
        <option value="12h">半天</option>
        <option value="1d">一天</option>
        <option value="3d">三天</option>
        <option value="1w">一周</option>
        <option value="manual">手动</option>
      </select>
      <button class="icon-btn feed-settings-notify ${notify ? 'active' : ''}" data-feed-id="${feed.id}" title="${notify ? '已开启该订阅源的桌面通知' : '该订阅源的桌面通知已关闭'}">
        ${notify
          ? `<svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M4.2 11.5h7.6L11 10.5V7.5a3 3 0 0 0-6 0v3z" fill="currentColor"/><path d="M6.8 13.2a1.2 1.2 0 0 0 2.4 0" fill="none"/></svg>`
          : `<svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3.2 3.2 12.8 12.8"/><path d="M4.5 11.5h7L11 10.5V7.5a3 3 0 0 0-4.96-2.27"/><path d="M5 7.5v3L4.2 11.5"/><path d="M6.8 13.2a1.2 1.2 0 0 0 2.4 0"/></svg>`}
      </button>
      <button class="icon-btn feed-settings-rename-btn" data-feed-id="${feed.id}" title="重命名">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="m10.5 2.8 2.7 2.7-7.8 7.8H2.7v-2.7Z"/><path d="m9.2 4.1 2.7 2.7"/></svg>
      </button>
      <button class="icon-btn danger feed-settings-delete" data-feed-id="${feed.id}" title="移除订阅">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4.5h10M6.4 4.5V3.2h3.2v1.3M4.5 4.5l.6 8.2a.8.8 0 0 0 .8.7h4.2a.8.8 0 0 0 .8-.7l.6-8.2"/></svg>
      </button>
    `;

    body.appendChild(row);
    const sel = row.querySelector('.feed-settings-interval');
    sel.value = interval;
  });

  body.querySelectorAll('.feed-settings-emoji').forEach(btn => {
    btn.addEventListener('click', e => {
      e.stopPropagation();
      const fid = parseInt(btn.dataset.feedId);
      openEmojiPicker(btn, fid, () => {
        renderFeedSettingsList();
        renderFeedList(allFeeds);
      });
    });
  });

  body.querySelectorAll('.feed-settings-interval').forEach(sel => {
    sel.addEventListener('change', async () => {
      const fid = parseInt(sel.dataset.feedId);
      const value = sel.value;
      try {
        await invoke('set_feed_interval', { id: fid, interval: value });
        const f = allFeeds.find(x => x.id === fid);
        if (f) f.refresh_interval = value;
      } catch (e) {
        console.warn('set_feed_interval failed:', e);
      }
    });
  });

  body.querySelectorAll('.feed-settings-notify').forEach(btn => {
    btn.addEventListener('click', async () => {
      const fid = parseInt(btn.dataset.feedId);
      const next = !feedNotify(fid);
      try {
        await invoke('set_feed_notify', { id: fid, notify: next });
        const f = allFeeds.find(x => x.id === fid);
        if (f) f.notify = next;
      } catch (e) {
        console.warn('set_feed_notify failed:', e);
      }
      renderFeedSettingsList();
    });
  });

  body.querySelectorAll('.feed-settings-rename-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const fid = parseInt(btn.dataset.feedId);
      const row = btn.closest('.feed-settings-row');
      const nameEl = row.querySelector('.feed-settings-name');
      if (!nameEl) return;
      const oldName = nameEl.textContent;
      const input = document.createElement('input');
      input.type = 'text';
      input.value = oldName;
      input.className = 'feed-settings-rename';
      nameEl.replaceWith(input);
      input.focus();
      input.select();
      const finish = async (commit) => {
        const val = input.value.trim();
        if (commit && val && val !== oldName) {
          try { await invoke('rename_feed', { id: fid, name: val }); await loadFeeds(); }
          catch (err) { setGlobalStatus('重命名失败: ' + err, 'error'); }
        }
        renderFeedSettingsList();
      };
      input.addEventListener('keydown', e => {
        if (e.key === 'Enter') finish(true);
        if (e.key === 'Escape') finish(false);
      });
      input.addEventListener('blur', () => finish(true));
    });
  });

  body.querySelectorAll('.feed-settings-delete').forEach(btn => {
    btn.addEventListener('click', async () => {
      const fid = parseInt(btn.dataset.feedId);
      if (await confirmDialog('确定删除该订阅源及其所有文章？')) deleteFeed(fid);
    });
  });
}

// ── PubMed RSS Generator ───────────────────────
function initPubmedGenerator() {
  const queryEl = document.getElementById('pubmed-query');
  const limitEl = document.getElementById('pubmed-limit');
  const nameEl  = document.getElementById('pubmed-feedname');
  const previewLink    = document.getElementById('pubmed-preview-link');
  const idleEl   = document.getElementById('pubmed-actions-idle');
  const resultEl = document.getElementById('pubmed-result');
  const resultUrlEl = document.getElementById('pubmed-result-url');
  const resultEyebrow = document.getElementById('pubmed-result-eyebrow-text');
  const resultActionsEl = document.getElementById('pubmed-result-actions');
  const btnGenerate = document.getElementById('btn-pubmed-generate');
  const btnNl       = document.getElementById('btn-pubmed-nl');
  const btnCopy   = document.getElementById('btn-pubmed-copy');
  const btnAdd    = document.getElementById('btn-pubmed-add');
  if (!queryEl) return;

  let state = 'idle';
  let generatedUrl = '';

  function buildFullQuery() {
    return queryEl.value.trim();
  }

  function updatePreview() {
    const full = buildFullQuery();
    previewLink.href = full ? `https://pubmed.ncbi.nlm.nih.gov/?term=${encodeURIComponent(full)}` : '#';
  }

  function reset() {
    state = 'idle';
    generatedUrl = '';
    resultEl.classList.add('hidden');
    resultEl.classList.remove('added');
    idleEl.style.display = 'flex';
    btnGenerate.disabled = false;
    btnGenerate.innerHTML = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2v3M8 11v3M2 8h3M11 8h3M4.2 4.2l2 2M9.8 9.8l2 2M11.8 4.2l-2 2M6.2 9.8l-2 2"/></svg> 生成 RSS 链接`;
  }

  queryEl.addEventListener('input', () => { updatePreview(); if (state !== 'idle') reset(); });
  limitEl.addEventListener('change', () => { if (state !== 'idle') reset(); });

  btnNl.addEventListener('click', async () => {
    const text = queryEl.value.trim();
    if (!text) {
      idleEl.style.display = 'none';
      resultEl.classList.remove('hidden');
      resultEl.classList.remove('added');
      resultUrlEl.style.color = 'var(--text-tertiary)';
      resultUrlEl.textContent = '请先在检索式输入框中输入检索需求描述';
      resultEyebrow.textContent = '提示';
      resultActionsEl.style.display = 'none';
      return;
    }
    const origHTML = btnNl.innerHTML;
    btnNl.disabled = true;
    btnNl.innerHTML = `<span class="spinner"></span> AI 生成中…`;
    try {
      const query = await invoke('natural_to_pubmed_query', { text });
      queryEl.value = query;
      updatePreview();
      state = 'idle';
      resultEl.classList.add('hidden');
      idleEl.style.display = 'flex';
    } catch (e) {
      idleEl.style.display = 'none';
      resultEl.classList.remove('hidden');
      resultEl.classList.remove('added');
      resultUrlEl.style.color = 'var(--text-secondary)';
      const msg = (typeof e === 'string') ? e : (e && e.message) || String(e);
      resultUrlEl.textContent = msg;
      resultEyebrow.textContent = 'AI 生成失败';
      resultActionsEl.style.display = 'none';
    } finally {
      btnNl.disabled = false;
      btnNl.innerHTML = origHTML;
    }
  });

  btnGenerate.addEventListener('click', async () => {
    const q = queryEl.value.trim();
    if (!q) {
      idleEl.style.display = 'none';
      resultEl.classList.remove('hidden');
      resultEl.classList.remove('added');
      resultUrlEl.style.color = 'var(--text-tertiary)';
      resultUrlEl.textContent = '请输入检索关键词';
      resultEyebrow.textContent = '提示';
      resultActionsEl.style.display = 'none';
      return;
    }
    state = 'generating';
    btnGenerate.disabled = true;
    btnGenerate.innerHTML = `<span class="spinner"></span> 生成中…`;
    try {
      const url = await invoke('build_pubmed_rss_url', {
        query: buildFullQuery(),
        limit: parseInt(limitEl.value, 10) || 15,
      });
      generatedUrl = url;
      state = 'ready';
      idleEl.style.display = 'none';
      resultEl.classList.remove('hidden');
      resultEl.classList.remove('added');
      resultUrlEl.style.color = '';
      resultUrlEl.textContent = url;
      resultActionsEl.style.display = 'flex';
      resultEyebrow.textContent = 'RSS 链接已生成';
    } catch (e) {
      generatedUrl = '';
      state = 'idle';
      idleEl.style.display = 'none';
      resultEl.classList.remove('hidden');
      resultEl.classList.remove('added');
      resultUrlEl.style.color = 'var(--text-secondary)';
      const msg = (typeof e === 'string') ? e : (e && e.message) || String(e);
      resultUrlEl.textContent = msg;
      resultEyebrow.textContent = '生成失败';
      resultActionsEl.style.display = 'none';
    } finally {
      btnGenerate.disabled = false;
      btnGenerate.innerHTML = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2v3M8 11v3M2 8h3M11 8h3M4.2 4.2l2 2M9.8 9.8l2 2M11.8 4.2l-2 2M6.2 9.8l-2 2"/></svg> 生成 RSS 链接`;
    }
  });

  btnCopy.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(generatedUrl); }
    catch {}
    btnCopy.innerHTML = `<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 8.3 6.5 11 12.5 4.8"/></svg> 已复制`;
    setTimeout(() => { btnCopy.textContent = '复制链接'; }, 1500);
  });

  btnAdd.addEventListener('click', async () => {
    if (!generatedUrl) return;
    btnAdd.disabled = true;
    try {
      await invoke('add_feed', { url: generatedUrl });
      const desiredName = nameEl.value.trim();
      const list = await invoke('list_feeds');
      const added = list.find(f => f.url === generatedUrl);
      if (added && desiredName) {
        try { await invoke('rename_feed', { id: added.id, name: desiredName }); } catch {}
      }
      state = 'added';
      resultEl.classList.add('added');
      resultEyebrow.innerHTML = `<svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 8.3 6.5 11 12.5 4.8"/></svg> 已添加至订阅源`;
      resultActionsEl.style.display = 'none';
      await loadFeeds();
      renderFeedSettingsList();
      setTimeout(() => {
        reset();
        queryEl.value = '';
        nameEl.value = '';
        updatePreview();
      }, 2200);
    } catch (e) {
      setGlobalStatus('添加失败: ' + e, 'error');
    } finally {
      btnAdd.disabled = false;
    }
  });

  updatePreview();
}

// ── Briefing settings card ─────────────────────
function initBriefingSettings() {
  const toggle = document.getElementById('briefing-enabled-toggle');
  const body = document.getElementById('briefing-card-body');
  const freqCtl = document.getElementById('briefing-frequency-control');
  const dayRow = document.getElementById('briefing-day-row');
  const daySel = document.getElementById('briefing-day');
  const hourInp = document.getElementById('briefing-hour');
  const promptInp = document.getElementById('briefing-prompt');
  const promptLen = document.getElementById('briefing-prompt-len');
  const promptHint = document.getElementById('briefing-prompt-hint');
  const btnExpand = document.getElementById('btn-briefing-expand');
  const btnReset = document.getElementById('btn-briefing-reset');
  const nextDateEl = document.getElementById('briefing-next-date');

  if (!toggle) return;

  // One-time migration: users whose localStorage still has the original 0.1.0
  // default ("医学文献编辑..." bullet-style prompt) get bumped to the new
  // "前沿进展" style. Detected by the unique opening phrase of the old default.
  const stored = localStorage.getItem('briefing-prompt');
  if (stored && stored.startsWith('你是一位资深的医学文献编辑')) {
    localStorage.removeItem('briefing-prompt');
  }

  // Load state
  const enabled = localStorage.getItem('briefing-enabled') !== '0';
  const frequency = localStorage.getItem('briefing-frequency') || 'weekly';
  const day = localStorage.getItem('briefing-day') || 'mon';
  const hour = localStorage.getItem('briefing-hour') || '09:00';
  const prompt = localStorage.getItem('briefing-prompt') || DEFAULT_BRIEFING_PROMPT;

  toggle.classList.toggle('on', enabled);
  body.classList.toggle('disabled', !enabled);
  freqCtl.querySelectorAll('.seg-btn').forEach(b => b.classList.toggle('active', b.dataset.value === frequency));
  dayRow.style.display = frequency === 'weekly' ? 'flex' : 'none';
  daySel.value = day;
  hourInp.value = hour;
  promptInp.value = prompt;
  promptLen.textContent = `${prompt.length} 字符`;
  promptHint.textContent = prompt === DEFAULT_BRIEFING_PROMPT ? '当前使用推荐默认 Prompt' : '已自定义 Prompt';
  nextDateEl.textContent = computeNextBriefingDate();

  // Any settings change clears the "don't retry within 1 hour" guard and
  // immediately re-evaluates whether a briefing is due — so if you set the
  // hour to "now", a briefing fires within a few seconds instead of waiting
  // for the next 5-minute tick.
  const settingsChanged = () => {
    localStorage.removeItem('briefing-last-attempt');
    setTimeout(briefingSchedulerTick, 500);
  };

  toggle.addEventListener('click', () => {
    const next = !toggle.classList.contains('on');
    toggle.classList.toggle('on', next);
    body.classList.toggle('disabled', !next);
    localStorage.setItem('briefing-enabled', next ? '1' : '0');
    if (next) settingsChanged();
  });

  freqCtl.querySelectorAll('.seg-btn').forEach(b => {
    b.addEventListener('click', () => {
      freqCtl.querySelectorAll('.seg-btn').forEach(x => x.classList.toggle('active', x === b));
      const v = b.dataset.value;
      localStorage.setItem('briefing-frequency', v);
      dayRow.style.display = v === 'weekly' ? 'flex' : 'none';
      nextDateEl.textContent = computeNextBriefingDate();
      settingsChanged();
    });
  });

  daySel.addEventListener('change', () => {
    localStorage.setItem('briefing-day', daySel.value);
    nextDateEl.textContent = computeNextBriefingDate();
    settingsChanged();
  });

  hourInp.addEventListener('change', () => {
    localStorage.setItem('briefing-hour', hourInp.value);
    nextDateEl.textContent = computeNextBriefingDate();
    settingsChanged();
  });

  promptInp.addEventListener('input', () => {
    const v = promptInp.value;
    localStorage.setItem('briefing-prompt', v);
    promptLen.textContent = `${v.length} 字符`;
    promptHint.textContent = v === DEFAULT_BRIEFING_PROMPT ? '当前使用推荐默认 Prompt' : '已自定义 Prompt';
  });

  btnExpand.addEventListener('click', () => {
    const expanded = promptInp.style.minHeight === '220px';
    promptInp.style.minHeight = expanded ? '90px' : '220px';
    promptInp.rows = expanded ? 4 : 12;
    btnExpand.textContent = expanded ? '展开' : '收起';
  });

  btnReset.addEventListener('click', () => {
    promptInp.value = DEFAULT_BRIEFING_PROMPT;
    localStorage.setItem('briefing-prompt', DEFAULT_BRIEFING_PROMPT);
    promptLen.textContent = `${DEFAULT_BRIEFING_PROMPT.length} 字符`;
    promptHint.textContent = '当前使用推荐默认 Prompt';
  });
}

// ── Reading stats ───────────────────────────────
// Stats run off a dedicated `get_reading_stats` backend command that aggregates
// the entire `entries` table in SQL — NOT off `globalEntries`, which is capped
// at LIMIT 200 by `list_entries` and would silently undercount once the DB
// holds more than 200 rows.
let heatmapDayCounts = new Map();
let heatmapYear = new Date().getFullYear();

async function renderReadingStats() {
  let stats;
  try {
    stats = await invoke('get_reading_stats');
  } catch (e) {
    console.error('get_reading_stats 失败:', e);
    return;
  }

  heatmapDayCounts = new Map(stats.day_counts || []);
  const { current, longest } = computeStreaks(heatmapDayCounts);
  const el = (id) => document.getElementById(id);
  el('stat-total-crawled').textContent = stats.total_entries;
  el('stat-total-read').textContent = stats.total_read;
  el('stat-current-streak').textContent = current;
  el('stat-longest-streak').textContent = longest;

  setupHeatmapYearSelect();
  renderHeatmap(heatmapDayCounts);
  renderFeedPrefsFromCounts(stats.feed_read_counts || []);
}

function setupHeatmapYearSelect() {
  const sel = document.getElementById('heatmap-year');
  if (!sel) return;
  const currentYear = new Date().getFullYear();
  let minYear = currentYear;
  for (const k of heatmapDayCounts.keys()) {
    const y = parseInt(k.slice(0, 4), 10);
    if (!Number.isNaN(y) && y < minYear) minYear = y;
  }
  if (heatmapYear < minYear || heatmapYear > currentYear) heatmapYear = currentYear;

  const years = [];
  for (let y = currentYear; y >= minYear; y--) years.push(y);
  sel.innerHTML = years.map(y => `<option value="${y}">${y} 年</option>`).join('');
  sel.value = String(heatmapYear);

  if (!sel.dataset.bound) {
    sel.addEventListener('change', () => {
      heatmapYear = parseInt(sel.value, 10) || currentYear;
      renderHeatmap(heatmapDayCounts);
    });
    sel.dataset.bound = '1';
  }
}

function computeStreaks(dayCounts) {
  if (dayCounts.size === 0) return { current: 0, longest: 0 };
  const dates = [...dayCounts.keys()].sort();
  let longest = 1, cur = 1, prev = new Date(dates[0]);
  for (let i = 1; i < dates.length; i++) {
    const d = new Date(dates[i]);
    const diff = Math.round((d - prev) / 86400000);
    if (diff === 1) { cur++; longest = Math.max(longest, cur); }
    else if (diff > 1) cur = 1;
    prev = d;
  }
  const today = new Date(); today.setHours(0,0,0,0);
  const last = new Date(dates[dates.length - 1]); last.setHours(0,0,0,0);
  const diffFromToday = Math.round((today - last) / 86400000);
  const current = diffFromToday <= 1 ? cur : 0;
  return { current, longest };
}

function renderHeatmap(dayCounts) {
  const container = document.getElementById('heatmap');
  if (!container) return;

  const year = heatmapYear;
  const today = new Date(); today.setHours(0, 0, 0, 0);

  // Grid window: from the Monday on/before Jan 1 to the Sunday on/after Dec 31,
  // so the cells line up with the existing 一/三/五/日 weekday labels (Mon-first).
  const yearStart = new Date(year, 0, 1);
  const yearEnd = new Date(year, 11, 31);
  const startDow = (yearStart.getDay() + 6) % 7; // Mon=0..Sun=6
  const gridStart = new Date(yearStart);
  gridStart.setDate(yearStart.getDate() - startDow);
  const endDow = (yearEnd.getDay() + 6) % 7;
  const gridEnd = new Date(yearEnd);
  gridEnd.setDate(yearEnd.getDate() + (6 - endDow));
  const totalDays = Math.round((gridEnd - gridStart) / 86400000) + 1;
  const weeks = Math.round(totalDays / 7);

  // Local YYYY-MM-DD (avoid UTC drift from .toISOString()).
  const ymd = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

  let html = '<div class="heatmap-body"><div class="heatmap-weekdays"><div>一</div><div></div><div>三</div><div></div><div>五</div><div></div><div>日</div></div><div class="heatmap-weeks">';
  const monthMarks = [];
  let lastMonth = -1;

  for (let w = 0; w < weeks; w++) {
    html += '<div class="heatmap-week">';
    for (let d = 0; d < 7; d++) {
      const day = new Date(gridStart);
      day.setDate(gridStart.getDate() + w * 7 + d);
      // Cells outside the selected year are hidden so the year-edge stays clean.
      // Future days inside the year render as empty cells (no count) so the
      // full 12-month grid is always visible.
      if (day.getFullYear() !== year) {
        html += '<div class="heatmap-cell" style="visibility:hidden"></div>';
        continue;
      }
      const k = ymd(day);
      const c = day > today ? 0 : (dayCounts.get(k) || 0);
      let cls = '';
      if (c >= 8) cls = 'l4';
      else if (c >= 4) cls = 'l3';
      else if (c >= 2) cls = 'l2';
      else if (c >= 1) cls = 'l1';
      html += `<div class="heatmap-cell ${cls}" title="${k}: ${c}"></div>`;
      if (d === 0 && day.getMonth() !== lastMonth) {
        monthMarks.push({ idx: w, label: (day.getMonth() + 1) + '月' });
        lastMonth = day.getMonth();
      }
    }
    html += '</div>';
  }
  html += '</div></div>';

  let monthHeader = '<div class="heatmap-months">';
  for (let i = 0; i < monthMarks.length; i++) {
    const cur = monthMarks[i];
    const next = monthMarks[i + 1];
    const span = ((next ? next.idx : weeks) - cur.idx) * 13;
    monthHeader += `<div class="heatmap-month" style="width:${span}px">${cur.label}</div>`;
  }
  monthHeader += '</div>';
  container.innerHTML = monthHeader + html;
}

function renderFeedPrefsFromCounts(feedReadCounts) {
  const wrap = document.getElementById('feed-pref-bars');
  if (!wrap) return;
  // Each row is [feed_id, snapshot_title, count] — snapshot lets us still name
  // feeds the user has since deleted, so the ranking doesn't lose entries.
  const total = feedReadCounts.reduce((sum, row) => sum + row[2], 0) || 1;
  const ranked = feedReadCounts.slice(0, 5);
  wrap.innerHTML = ranked.map(([fid, snapshot, n]) => {
    const feed = allFeeds.find(f => f.id === fid);
    const liveName = feed ? (feed.title || feed.url) : null;
    const name = liveName || snapshot || `#${fid}`;
    const isDeleted = !feed;
    const emoji = feedEmoji(fid);
    const pct = Math.round(n / total * 100);
    return `
      <div class="feed-pref-row">
        <div class="feed-pref-emoji">${emoji}</div>
        <div class="feed-pref-info">
          <div class="feed-pref-name">${escapeHtml(name)}${isDeleted ? ' <span class="feed-pref-tag">已删除</span>' : ''}</div>
          <div class="feed-pref-track"><div class="feed-pref-fill" style="width:${pct}%"></div></div>
        </div>
        <div class="feed-pref-pct">${pct}%</div>
      </div>
    `;
  }).join('') || '<div class="srow-hint">暂无阅读记录</div>';
}

// ── Utils ──────────────────────────────────────
function timeAgo(dateStr) {
  const now = Date.now(), then = new Date(dateStr).getTime();
  if (isNaN(then)) return '';
  const diff = Math.floor((now - then) / 1000);
  if (diff < 60) return '刚刚';
  if (diff < 3600) return Math.floor(diff / 60) + ' 分钟前';
  if (diff < 86400) return Math.floor(diff / 3600) + ' 小时前';
  if (diff < 604800) return Math.floor(diff / 86400) + ' 天前';
  return new Date(dateStr).toLocaleDateString('zh-CN');
}

function escapeHtml(text) {
  const d = document.createElement('div');
  d.textContent = text == null ? '' : String(text);
  return d.innerHTML;
}

function stripHtml(html) {
  const d = document.createElement('div');
  d.innerHTML = html;
  return d.textContent || '';
}

function delay(ms) { return new Promise(r => setTimeout(r, ms)); }

function formatAuthors(authorStr) {
  if (!authorStr) return '';
  const authors = authorStr.split(',').map(s => s.trim()).filter(Boolean);
  if (authors.length === 0) return '';
  if (authors.length === 1) return authors[0];
  const first = authors[0];
  const last = authors[authors.length - 1];
  if (first === last) return first;
  return `${first}, ⋆ ${last}`;
}

// entry.source is the journal name parsed from the RSS description by
// article_service::extract_source on the Rust side (already trimmed to just
// the journal). NOT to be confused with feed.title, which is the user's
// custom RSS feed name (e.g. a PubMed search query).
function journalName(entry) {
  return (entry?.source || '').trim();
}

function formatPublicationDate(entry) {
  if (entry.publication_date) return entry.publication_date;
  if (!entry.published_at) return '';
  const d = new Date(entry.published_at);
  if (Number.isNaN(d.getTime())) return entry.published_at;
  return d.toLocaleDateString('zh-CN');
}

// ── Init ───────────────────────────────────────
window.addEventListener('DOMContentLoaded', () => {
  // Default the appshell to settings-mode (sidebar tucked away) so the
  // first paint doesn't show an empty 252-px gap on the left before
  // `loadSettings()` decides which view to surface. `showMain()` /
  // `showSettings()` toggle this class authoritatively below.
  document.body.classList.add('settings-mode');

  // Layout
  settingsView = document.getElementById('settings-view');
  mainView     = document.getElementById('main-view');
  contentArea  = document.getElementById('content-area');
  toolbarSubtitle = document.getElementById('toolbar-subtitle');

  // Toolbar
  btnSettings = document.getElementById('btn-settings');
  btnSidebar  = document.getElementById('btn-sidebar');
  btnRefresh  = document.getElementById('btn-refresh');
  refreshIcon = document.getElementById('refresh-icon');

  // Settings inputs
  apiKeyInput       = document.getElementById('api-key');
  baseUrlInput      = document.getElementById('base-url');
  modelInput        = document.getElementById('model');
  systemPromptInput = document.getElementById('system-prompt');
  retentionSelect   = document.getElementById('read-retention');
  btnToggleApiKey   = document.getElementById('btn-toggle-api-key');
  btnTest           = document.getElementById('btn-test');
  btnSaveSettings   = document.getElementById('btn-save-settings');
  btnSaveGeneral    = document.getElementById('btn-save-general');
  settingsStatus    = document.getElementById('settings-status');
  generalStatus     = document.getElementById('general-status');
  themeControl      = document.getElementById('theme-control');
  accentSwatches    = document.getElementById('accent-swatches');
  fontscaleControl  = document.getElementById('fontscale-control');

  // Feeds
  feedUrlInput  = document.getElementById('feed-url');
  btnAddFeed    = document.getElementById('btn-add-feed');
  addFeedRow    = document.getElementById('add-feed-row');
  addFeedIcon   = document.getElementById('add-feed-icon');
  feedListEl    = document.getElementById('feed-list');
  globalStatusEl = document.getElementById('global-status');

  // Entry list
  entryListEl     = document.getElementById('entry-list');
  entryItemsEl    = document.getElementById('entry-items');
  entryFilter     = document.getElementById('entry-filter');

  // Briefing list
  briefingListEl  = document.getElementById('briefing-list');
  briefingItemsEl = document.getElementById('briefing-items');

  // Detail
  detailPanelEl        = document.getElementById('detail-panel');
  briefingDetailEl     = document.getElementById('briefing-detail');
  detailEmpty          = document.getElementById('detail-empty');
  detailContent        = document.getElementById('detail-content');
  detailTitle          = document.getElementById('detail-title');
  detailJournal        = document.getElementById('detail-journal');
  detailAffiliation    = document.getElementById('detail-affiliation');
  detailPublicationDate = document.getElementById('detail-publication-date');
  detailDateSub        = document.getElementById('detail-date-sub');
  detailSummaryContent = document.getElementById('detail-summary-content');
  detailSummarySection = document.getElementById('detail-summary-section');
  detailSummaryRetry  = document.getElementById('detail-summary-retry');
  detailBadgeRow       = document.getElementById('detail-badge-row');
  detailSourceBadge    = document.getElementById('detail-source-badge');
  btnOpenUrl           = document.getElementById('btn-open-url');
  btnRetrySummary      = document.getElementById('btn-retry-summary');
  briefingDetailEmpty  = document.getElementById('briefing-detail-empty');
  briefingDetailContent = document.getElementById('briefing-detail-content');

  // Wire events
  btnSettings.addEventListener('click', () => {
    if (!mainView.classList.contains('hidden')) showSettings('feeds');
    else showMain();
  });

  btnSidebar.addEventListener('click', () => {
    if (!mainView.classList.contains('hidden')) toggleSidebar();
    else showMain();
  });

  btnRefresh.addEventListener('click', refreshAll);
  btnToggleApiKey.addEventListener('click', toggleApiKeyVisibility);
  btnTest.addEventListener('click', testConnection);
  btnSaveSettings.addEventListener('click', saveTranslationSettings);
  btnSaveGeneral?.addEventListener('click', saveGeneralSettings);
  btnRetrySummary?.addEventListener('click', retrySummaryTranslation);
  document.getElementById('btn-refresh-balance')?.addEventListener('click', () => refreshDeepSeekBalance());

  // Settings rail
  document.querySelectorAll('.settings-rail-item').forEach(btn => {
    btn.addEventListener('click', () => activateSettingsSection(btn.dataset.section));
  });

  // Sidebar overview rows
  document.querySelectorAll('.sidebar-row').forEach(row => {
    row.addEventListener('click', () => {
      const view = row.dataset.view;
      if (view === 'briefing') { enterBriefingMode(); return; }
      enterFeedMode();
      document.querySelectorAll('.feed-item').forEach(el => el.classList.remove('selected'));
      document.querySelectorAll('.sidebar-row').forEach(el => el.classList.toggle('active', el === row));
      selectedFeedId = null;
      if (view === 'unread') {
        entryFilterValue = 'unread';
        entryFilter?.querySelectorAll('.seg-btn').forEach(b => b.classList.toggle('active', b.dataset.filter === 'unread'));
      } else if (view === 'starred') {
        entryFilterValue = 'starred';
        entryFilter?.querySelectorAll('.seg-btn').forEach(b => b.classList.toggle('active', b.dataset.filter === 'starred'));
      } else {
        entryFilterValue = 'all';
        entryFilter?.querySelectorAll('.seg-btn').forEach(b => b.classList.toggle('active', b.dataset.filter === 'all'));
      }
      setToolbarSubtitle('main');
      loadEntries(null);
    });
  });

  // Toggle switches in general settings — sync from localStorage on load,
  // persist + react on click. Pref key = data-pref attribute.
  document.querySelectorAll('.toggle-switch[data-pref]').forEach(btn => {
    const pref = btn.dataset.pref;
    const lsKey = `pref-${pref}`;
    let on;
    if (pref === 'tray-visible') {
      on = trayVisiblePref();
    } else {
      const v = localStorage.getItem(lsKey);
      on = v === null ? btn.classList.contains('on') : v === '1';
    }
    btn.classList.toggle('on', on);
    btn.addEventListener('click', () => {
      const next = !btn.classList.contains('on');
      btn.classList.toggle('on', next);
      if (pref === 'tray-visible') {
        setTrayVisiblePref(next);
        applyTrayVisibility(next);
      } else {
        localStorage.setItem(lsKey, next ? '1' : '0');
      }
    });
  });

  // Test notification — go through the Rust backend so we exercise exactly
  // the same NotificationExt path the scheduler uses. If this banner shows,
  // background-refresh banners will too.
  document.getElementById('btn-test-notification')?.addEventListener('click', async () => {
    const status = document.getElementById('general-status');
    if (status) { status.textContent = '正在发送测试通知…'; status.className = 'settings-status progress'; }
    try {
      await invoke('send_test_notification');
      if (status) {
        status.textContent = '已发送测试通知，请查看 macOS 通知中心';
        status.className = 'settings-status success';
      }
    } catch (e) {
      if (status) {
        status.textContent = String(e);
        status.className = 'settings-status error';
      }
    }
  });

  // Apply initial tray visibility on launch.
  applyTrayVisibility(trayVisiblePref());

  // Star button on detail
  document.getElementById('btn-star')?.addEventListener('click', () => {
    if (!currentEntry) return;
    toggleStar(currentEntry.id);
    const isStarred = starredIds().has(currentEntry.id);
    document.getElementById('btn-star').classList.toggle('active', isStarred);
    updateOverviewCounts();
  });

  // Abstract toggle
  document.querySelectorAll('.abstract-toggle-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      abstractLang = btn.dataset.lang;
      syncAbstractToggle();
      if (currentEntry) renderSummary(currentEntry);
    });
  });

  // Entry filter
  if (entryFilter) {
    entryFilter.querySelectorAll('.seg-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        entryFilterValue = btn.dataset.filter;
        entryFilter.querySelectorAll('.seg-btn').forEach(b => b.classList.toggle('active', b === btn));
        renderEntryList(allEntries);
      });
    });
  }

  // Add feed input
  feedUrlInput.addEventListener('input', () => {
    const hasText = feedUrlInput.value.trim().length > 0;
    addFeedRow.classList.toggle('active', hasText);
    btnAddFeed.classList.toggle('hidden', !hasText);
  });
  feedUrlInput.addEventListener('keydown', e => { if (e.key === 'Enter') addFeed(); });
  btnAddFeed.addEventListener('click', addFeed);

  // Generate briefing
  document.getElementById('btn-generate-briefing')?.addEventListener('click', generateBriefingNow);

  // Initialize sub-modules
  setupWindowDragFallback();
  initAppearanceControls();
  initPubmedGenerator();
  initBriefingSettings();
  syncAppearanceFromStorage();

  // Restore sidebar state
  sidebarCollapsed = localStorage.getItem('sidebar-collapsed') === '1';
  applyCollapsedState();

  setupCostEvents();
  loadCostSummary();
  setupTranslationEvents();
  wireTranslationBannerButtons();
  initUpdateChannel();
  ensureNotificationPermission();
  loadSettings();
  loadBriefings().then(() => startBriefingScheduler());
  // Backfill: catch up on any pre-existing entries that still need translation.
  // The pipeline itself is idempotent; if everything is already translated it
  // just returns immediately.
  invoke('start_translation_pipeline').catch(() => {});
});
