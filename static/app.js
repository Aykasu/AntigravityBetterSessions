// Antigravity Sentinel • Material You Expressive Dashboard Client

let dailyChart = null;
let projectsChart = null;
let rawStats = null;
let allProjects = [];
let allConversations = [];
let lastGemini5hFraction = null;

// Account & Sync State
let currentAccountFilter = 'all';
let accountsList = [];
let renameTargetId = null;
let lastSyncTimestamp = Date.now();
let lastTotalTokens = null;
let sessionStartTokens = null;
let tokenBurnHistory = [];

// Switch Account Filter
function switchAccount(acc) {
  currentAccountFilter = acc;
  renderAccountSwitcher();
  renderQuotas();

  if (rawStats) {
    applyAccountFilterAndRender(rawStats);
  }
}

// Fetch Accounts from Backend Profile Store
async function fetchAccounts() {
  try {
    const res = await fetch('/api/accounts');
    if (!res.ok) return;
    accountsList = await res.json();
    renderAccountSwitcher();
  } catch (e) {
    console.error('fetchAccounts error:', e);
  }
}

// Render dynamic account switcher
function renderAccountSwitcher() {
  const container = document.getElementById('account-switcher');
  if (!container) return;

  let html = `<button class="m3-acc-btn ${currentAccountFilter === 'all' ? 'active' : ''}" onclick="switchAccount('all')">Все</button>`;

  for (const acc of accountsList) {
    const isSelected = currentAccountFilter.toLowerCase() === acc.id.toLowerCase();
    const label = escapeHtml(acc.alias || acc.name || acc.email || 'Новый профиль');
    const dotClass = acc.isActive ? 'acc-dot active' : 'acc-dot';
    const dotTitle = acc.isActive ? 'Активен сейчас в Antigravity' : `Был в сети: ${acc.lastSeen ? acc.lastSeen.slice(0, 10) : 'ранее'}`;
    const launchTitle = `Запустить Antigravity с этим аккаунтом (${label})`;

    html += `
      <button class="m3-acc-btn ${isSelected ? 'active' : ''}" onclick="switchAccount('${escapeHtml(acc.id)}')">
        <span class="${dotClass}" title="${dotTitle}"></span>
        <span>${label}</span>
        <span class="acc-action-btn" title="${launchTitle}" onclick="launchProfile('${escapeHtml(acc.id)}', event)">🚀</span>
        <span class="acc-action-btn" title="Настройки / переименовать" onclick="openRenameModal('${escapeHtml(acc.id)}', '${escapeHtml(acc.alias || '')}', '${escapeHtml(acc.email)}', event)">✏️</span>
      </button>
    `;
  }

  html += `
    <button class="m3-acc-add-btn" title="Добавить новый профиль / аккаунт" onclick="openNewProfileModal(event)">+</button>
  `;

  container.innerHTML = html;
}

// Profile Launch Helper
async function launchProfile(id, event) {
  if (event) event.stopPropagation();
  const acc = accountsList.find(a => a.id === id);
  const name = acc ? (acc.alias || acc.name || acc.email) : id;
  showToast(`Перезапуск Antigravity с профилем "${name}"... 🚀`);
  try {
    const res = await fetch('/api/profiles/launch', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id })
    });
    if (res.ok) {
      triggerSyncPulse();
      setTimeout(fetchLiveQuota, 2500);
      setTimeout(fetchStats, 3000);
    } else {
      const err = await res.text();
      showToast('Не удалось запустить Antigravity: ' + err, true);
    }
  } catch (e) {
    console.error('Launch profile error:', e);
  }
}

// New Profile Modal Helpers
function openNewProfileModal(event) {
  if (event) event.stopPropagation();
  const modal = document.getElementById('new-profile-modal');
  const input = document.getElementById('new-profile-alias');
  if (input) input.value = '';
  if (modal) modal.style.display = 'flex';
  if (input) setTimeout(() => input.focus(), 50);
}

function closeNewProfileModal() {
  const modal = document.getElementById('new-profile-modal');
  if (modal) modal.style.display = 'none';
}

async function submitCreateProfile(andLaunch = false) {
  const input = document.getElementById('new-profile-alias');
  const alias = input ? input.value.trim() : '';
  if (!alias) {
    alert('Пожалуйста, введите название профиля (например: "Второй акк" или "Рабочий")');
    return;
  }

  try {
    const res = await fetch('/api/profiles/create', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ alias })
    });

    if (!res.ok) {
      const err = await res.text();
      alert('Ошибка создания профиля: ' + err);
      return;
    }

    const created = await res.json();
    closeNewProfileModal();
    await fetchAccounts();
    switchAccount(created.id);

    if (andLaunch && created.id) {
      await launchProfile(created.id);
    }
  } catch (e) {
    console.error('Create profile error:', e);
  }
}

// Rename Modal Helpers
function openRenameModal(id, currentAlias, email, event) {
  if (event) event.stopPropagation();
  renameTargetId = id;
  const modal = document.getElementById('rename-modal');
  const input = document.getElementById('rename-input');
  const emailLabel = document.getElementById('rename-email-label');
  const delBtn = document.getElementById('rename-delete-btn');

  const acc = accountsList.find(a => a.id === id);
  if (delBtn) {
    // Show delete button for custom profiles or if there are multiple accounts
    delBtn.style.display = (acc && (acc.dataDir || accountsList.length > 1)) ? 'inline-block' : 'none';
  }

  if (emailLabel) emailLabel.textContent = email ? `Почта: ${email}` : `Профиль: ${currentAlias || id}`;
  if (input) input.value = currentAlias || '';
  if (modal) modal.style.display = 'flex';
  if (input) setTimeout(() => input.focus(), 50);
}

function closeRenameModal() {
  const modal = document.getElementById('rename-modal');
  if (modal) modal.style.display = 'none';
  renameTargetId = null;
}

async function submitDeleteProfile() {
  if (!renameTargetId) return;
  if (!confirm('Вы уверены, что хотите удалить этот профиль из списка Sentinel?')) return;

  try {
    const res = await fetch('/api/profiles/delete', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: renameTargetId })
    });
    if (res.ok) {
      closeRenameModal();
      currentAccountFilter = 'all';
      await fetchAccounts();
      if (rawStats) applyAccountFilterAndRender(rawStats);
    }
  } catch (e) {
    console.error('Delete error:', e);
  }
}

async function submitAccountRename() {
  if (!renameTargetId) return;
  const input = document.getElementById('rename-input');
  const alias = input ? input.value.trim() : '';
  try {
    const res = await fetch('/api/accounts/rename', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: renameTargetId, alias })
    });
    if (res.ok) {
      closeRenameModal();
      await fetchAccounts();
      if (rawStats) applyAccountFilterAndRender(rawStats);
    }
  } catch (e) {
    console.error('Rename error:', e);
  }
}

// Tab Switching
function switchTab(targetTabId) {
  const tabs = document.querySelectorAll('.m3-nav-pill');
  const panes = document.querySelectorAll('.tab-pane');

  tabs.forEach(btn => {
    btn.classList.toggle('active', btn.getAttribute('data-tab') === targetTabId);
  });

  panes.forEach(pane => {
    pane.classList.toggle('active', pane.id === targetTabId);
  });

  if (targetTabId === 'tab-analytics') {
    if (rawStats) {
      renderCharts(rawStats, allProjects);
    }
    setTimeout(() => {
      if (dailyChart) dailyChart.resize();
      if (projectsChart) projectsChart.resize();
    }, 30);
  }
}

// Audio Chime & Sound Alert State
let soundAlertEnabled = true;
try {
  const savedSound = localStorage.getItem('antigravity_sound_alert');
  if (savedSound !== null) {
    soundAlertEnabled = savedSound === 'true';
  }
} catch (e) {}

function toggleSoundAlert() {
  soundAlertEnabled = !soundAlertEnabled;
  try {
    localStorage.setItem('antigravity_sound_alert', soundAlertEnabled);
  } catch (e) {}
  updateSoundAlertUI();
}

function updateSoundAlertUI() {
  const btn = document.getElementById('sound-alert-btn');
  if (!btn) return;
  if (soundAlertEnabled) {
    btn.classList.add('active');
    btn.title = 'Звуковые оповещения включены';
    btn.innerHTML = '<i data-lucide="bell" id="sound-alert-icon"></i>';
  } else {
    btn.classList.remove('active');
    btn.title = 'Звуковые оповещения выключены';
    btn.innerHTML = '<i data-lucide="bell-off" id="sound-alert-icon"></i>';
  }
  if (window.lucide) lucide.createIcons();
}

// Audio Chime
function playAlertSound() {
  if (!soundAlertEnabled) return;

  try {
    const ctx = new (window.AudioContext || window.webkitAudioContext)();
    const now = ctx.currentTime;

    const osc1 = ctx.createOscillator();
    const gain1 = ctx.createGain();
    osc1.type = 'sine';
    osc1.frequency.setValueAtTime(659.25, now);
    gain1.gain.setValueAtTime(0.18, now);
    gain1.gain.exponentialRampToValueAtTime(0.001, now + 0.35);
    osc1.connect(gain1);
    gain1.connect(ctx.destination);
    osc1.start(now);
    osc1.stop(now + 0.35);

    const osc2 = ctx.createOscillator();
    const gain2 = ctx.createGain();
    osc2.type = 'sine';
    osc2.frequency.setValueAtTime(830.61, now + 0.12);
    gain2.gain.setValueAtTime(0.22, now + 0.12);
    gain2.gain.exponentialRampToValueAtTime(0.001, now + 0.55);
    osc2.connect(gain2);
    gain2.connect(ctx.destination);
    osc2.start(now + 0.12);
    osc2.stop(now + 0.55);
  } catch (e) {
    console.warn('Audio error:', e);
  }
}

function formatNumber(num) {
  if (num === null || num === undefined) return '0';
  return Number(num).toLocaleString('ru-RU');
}

function formatCompactTokens(num) {
  if (num === null || num === undefined || isNaN(num)) return '0';
  const n = Number(num);
  if (n >= 1_000_000_000) {
    return (n / 1_000_000_000).toFixed(2) + 'B';
  }
  if (n >= 1_000_000) {
    return (n / 1_000_000).toFixed(1) + 'M';
  }
  if (n >= 1_000) {
    return (n / 1_000).toFixed(1) + 'K';
  }
  return n.toLocaleString('ru-RU');
}

function formatTokenEstimate(tokens) {
  if (tokens === null || tokens === undefined || isNaN(tokens)) return '--';
  const t = Math.max(0, Math.round(tokens));
  if (t >= 1_000_000) {
    const val = (t / 1_000_000).toFixed(1);
    return `~${val.endsWith('.0') ? val.slice(0, -2) : val}M`;
  }
  if (t >= 1_000) {
    const val = (t / 1_000).toFixed(0);
    return `~${val}K`;
  }
  return `~${t}`;
}

function formatDuration(seconds) {
  if (!seconds || seconds <= 0) return '0м';
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  if (hrs > 0) {
    return `${hrs}ч ${mins}м`;
  }
  return `${mins}м`;
}

function formatCountdown(resetTimeStr) {
  if (!resetTimeStr) return '--:--';
  const resetDate = new Date(resetTimeStr).getTime();
  const now = Date.now();
  const diffMs = resetDate - now;

  if (diffMs <= 0) {
    return '00:00';
  }

  const totalSecs = Math.floor(diffMs / 1000);
  const days = Math.floor(totalSecs / 86400);
  const hours = Math.floor((totalSecs % 86400) / 3600);
  const minutes = Math.floor((totalSecs % 3600) / 60);
  const seconds = totalSecs % 60;

  if (days > 0) {
    return `${days}д ${hours}ч`;
  }
  if (hours > 0) {
    return `${hours}ч ${minutes}м`;
  }
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}

function formatExactReset(resetTimeStr) {
  if (!resetTimeStr) return '';
  const resetDate = new Date(resetTimeStr);
  if (isNaN(resetDate.getTime())) return '';

  const now = new Date();
  const isToday = resetDate.toDateString() === now.toDateString();
  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);
  const isTomorrow = resetDate.toDateString() === tomorrow.toDateString();

  const hours = String(resetDate.getHours()).padStart(2, '0');
  const minutes = String(resetDate.getMinutes()).padStart(2, '0');
  const timeStr = `${hours}:${minutes}`;

  if (isToday) {
    return `сегодня в ${timeStr}`;
  } else if (isTomorrow) {
    return `завтра в ${timeStr}`;
  } else {
    const months = ['янв', 'фев', 'мар', 'апр', 'мая', 'июн', 'июл', 'авг', 'сен', 'окт', 'ноя', 'дек'];
    return `${resetDate.getDate()} ${months[resetDate.getMonth()]} в ${timeStr}`;
  }
}

// Account Quota Cache
let accountQuotas = {};
try {
  accountQuotas = JSON.parse(localStorage.getItem('antigravity_account_quotas') || '{}');
} catch (e) {}

let rawLiveQuota = null;

// Live Quotas API
async function fetchLiveQuota() {
  try {
    const res = await fetch('/api/live');
    if (!res.ok) return;
    const data = await res.json();
    rawLiveQuota = data;

    triggerSyncPulse();

    const connBadge = document.getElementById('connection-status');
    const tierName = document.getElementById('tier-name');
    const activeAccEl = document.getElementById('live-active-account');

    if (data.connected) {
      if (connBadge) {
        connBadge.className = 'm3-chip online';
        connBadge.title = `В сети • ${data.user_email || data.user_name || ''} • порт ${data.port}`;
      }
      if (tierName) {
        tierName.textContent = data.user_tier?.name || 'В сети';
      }

      const email = data.user_email || '';
      const name = data.user_name || '';
      const accProfile = accountsList.find(a => 
        (email && a.email.toLowerCase() === email.toLowerCase()) ||
        (name && a.name.toLowerCase() === name.toLowerCase())
      );

      if (activeAccEl) {
        activeAccEl.textContent = accProfile && accProfile.alias ? accProfile.alias : (name || email);
      }

      // Update accounts list if needed
      fetchAccounts();
    } else {
      if (connBadge) {
        connBadge.className = 'm3-chip offline';
        connBadge.title = 'Antigravity IDE не запущен';
      }
      if (tierName) {
        tierName.textContent = 'Оффлайн';
      }
      if (activeAccEl) {
        activeAccEl.textContent = '—';
      }
    }

    renderQuotas();
  } catch (e) {
    console.error(e);
  }
}

function renderQuotas() {
  let targetData = rawLiveQuota;
  let isCachedSnapshot = false;

  if (currentAccountFilter !== 'all') {
    const accProfile = accountsList.find(a => 
      a.id.toLowerCase() === currentAccountFilter.toLowerCase() ||
      (a.email && a.email.toLowerCase() === currentAccountFilter.toLowerCase())
    );
    if (accProfile) {
      if (accProfile.isActive && rawLiveQuota && rawLiveQuota.connected) {
        targetData = rawLiveQuota;
      } else if (accProfile.lastQuota) {
        targetData = { quota: accProfile.lastQuota };
        isCachedSnapshot = true;
      }
    }
  }

  if (!targetData || !targetData.quota || !targetData.quota.groups) return;

  for (const grp of targetData.quota.groups) {
    const isGemini = grp.displayName.toLowerCase().includes('gemini');
    const prefix = isGemini ? 'gemini' : 'claude';

    let fraction5h = null;
    let fractionWeekly = null;

    for (const bucket of grp.buckets || []) {
      const is5h = bucket.window === '5h' || bucket.bucketId.includes('5h');
      const suffix = is5h ? '5h' : 'weekly';
      const percent = Math.round(bucket.remainingFraction * 100);

      if (is5h) fraction5h = bucket.remainingFraction;
      else fractionWeekly = bucket.remainingFraction;

      const percentEl = document.getElementById(`${prefix}-${suffix}-percent`);
      const barEl = document.getElementById(`${prefix}-${suffix}-bar`);
      const timerEl = document.getElementById(`${prefix}-${suffix}-timer`);
      const exactEl = document.getElementById(`${prefix}-${suffix}-exact`);

      if (percentEl) percentEl.textContent = `${percent}%`;
      if (barEl) barEl.style.width = `${percent}%`;

      // Token capacity estimate based on Google AI Pro Tier benchmarks
      const baseCapacity = isGemini ? (is5h ? 2_000_000 : 6_000_000) : (is5h ? 750_000 : 2_250_000);
      const estTokens = Math.round(bucket.remainingFraction * baseCapacity);
      const availEl = document.getElementById(`${prefix}-${suffix}-tokens-avail`);
      if (availEl) {
        availEl.textContent = `${formatTokenEstimate(estTokens)} доступно`;
        availEl.title = `Ориентировочно доступно: ~${formatNumber(estTokens)} токенов (из ~${formatCompactTokens(baseCapacity)})`;
      }

      if (timerEl) {
        const countdown = formatCountdown(bucket.resetTime);
        timerEl.querySelector('span').textContent = countdown;
      }

      if (exactEl) {
        const exactStr = formatExactReset(bucket.resetTime);
        exactEl.textContent = exactStr ? `сброс: ${exactStr}` : '--';
      }

      if (is5h) {
        const statusEl = document.getElementById(`${prefix}-5h-status`);
        if (statusEl) {
          if (isCachedSnapshot) {
            statusEl.className = 'm3-pill-status cached-status';
            statusEl.textContent = 'Снимок';
          } else if (percent === 0) {
            statusEl.className = 'm3-pill-status limited';
            statusEl.textContent = 'Исчерпан';
          } else if (percent < 20) {
            statusEl.className = 'm3-pill-status limited';
            statusEl.textContent = '< 20%';
          } else {
            statusEl.className = 'm3-pill-status';
            statusEl.textContent = 'Доступно';
          }
        }

        if (isGemini && !isCachedSnapshot) {
          if (lastGemini5hFraction !== null && lastGemini5hFraction === 0 && bucket.remainingFraction > 0) {
            playAlertSound();
          }
          lastGemini5hFraction = bucket.remainingFraction;
        }
      }
    }

    // Projected impact of consuming 5h quota onto weekly limit
    if (fraction5h !== null && fractionWeekly !== null) {
      // In Antigravity, 1 full 5h quota drains ~33.3% of the weekly allowance
      const FULL_5H_DRAIN_RATIO = 0.333;
      const weeklyDrain = fraction5h * FULL_5H_DRAIN_RATIO;
      const weeklyProjected = Math.max(0, fractionWeekly - weeklyDrain);

      const projPercent = Math.round(weeklyProjected * 100);
      const drainPercent = Math.round(weeklyDrain * 100);

      const projEl = document.getElementById(`${prefix}-weekly-projected`);
      if (projEl) {
        projEl.textContent = `→ ~${projPercent}%`;
      }

      const impactEl = document.getElementById(`${prefix}-weekly-impact`);
      if (impactEl) {
        impactEl.textContent = `-${drainPercent}% за 5ч`;
      }

      // Ghost striped overlay inside the weekly progress bar
      const ghostEl = document.getElementById(`${prefix}-weekly-ghost`);
      if (ghostEl) {
        if (fractionWeekly > 0) {
          const ghostWidthFrac = Math.min(1.0, weeklyDrain / fractionWeekly);
          ghostEl.style.width = `${Math.round(ghostWidthFrac * 100)}%`;
        } else {
          ghostEl.style.width = '0%';
        }
      }
    }
  }

  updateMiniHudUI();
}

// Database Stats API
async function fetchStats() {
  try {
    const res = await fetch('/api/stats');
    if (!res.ok) return;
    const stats = await res.json();
    rawStats = stats;

    triggerSyncPulse();
    applyAccountFilterAndRender(stats);
  } catch (e) {
    console.error(e);
  }
}

// Session metrics tracking
let sessionTokensStartByAccount = {};
let lastKnownTokensByAccount = {};
let lastStepDeltaByAccount = {};

// Smooth Number Counter Animation
function animateNumber(elementId, targetVal) {
  const el = document.getElementById(elementId);
  if (!el) return;
  const currentStr = el.textContent.replace(/\s/g, '').replace(/,/g, '');
  const currentVal = parseInt(currentStr, 10) || 0;

  if (currentVal === targetVal) {
    el.textContent = formatNumber(targetVal);
    return;
  }

  const diff = targetVal - currentVal;
  if (Math.abs(diff) > 0 && Math.abs(diff) < 2000000) {
    let start = null;
    const duration = 150; // Snappy 150ms animation
    function step(timestamp) {
      if (!start) start = timestamp;
      const progress = Math.min((timestamp - start) / duration, 1);
      const val = Math.floor(currentVal + diff * progress);
      el.textContent = formatNumber(val);
      if (progress < 1) {
        requestAnimationFrame(step);
      } else {
        el.textContent = formatNumber(targetVal);
      }
    }
    requestAnimationFrame(step);
  } else {
    el.textContent = formatNumber(targetVal);
  }
}

// Filter stats by selected account and render
function applyAccountFilterAndRender(stats) {
  let filteredProjects = stats.projects || [];
  let filteredConversations = stats.conversations || [];

  if (currentAccountFilter !== 'all') {
    const filterLower = currentAccountFilter.toLowerCase();
    filteredProjects = filteredProjects.filter(p => (p.account || '').toLowerCase().includes(filterLower));
    filteredConversations = filteredConversations.filter(c => (c.account || '').toLowerCase().includes(filterLower));
  }

  // Calculate totals for filtered view
  let inTokens = 0;
  let outTokens = 0;
  let durationSecs = 0;
  let totalSteps = 0;

  for (const p of filteredProjects) {
    inTokens += p.total_input_tokens || 0;
    outTokens += p.total_output_tokens || 0;
    durationSecs += p.total_duration_seconds || 0;
    totalSteps += p.total_steps || 0;
  }
  const totalTokens = inTokens + outTokens;

  // Track session and step metrics
  updateMetricsStrip(totalTokens, stats, filteredProjects);

  // Fast number updates
  animateNumber('kpi-total-tokens', totalTokens);
  document.getElementById('kpi-in-tokens').textContent = formatNumber(inTokens);
  document.getElementById('kpi-out-tokens').textContent = formatNumber(outTokens);
  document.getElementById('kpi-total-time').textContent = formatDuration(durationSecs);
  document.getElementById('kpi-projects-count').textContent = filteredProjects.length;
  document.getElementById('kpi-steps-count').textContent = formatNumber(totalSteps);

  allProjects = filteredProjects;
  allConversations = filteredConversations;

  renderQuickProjects(filteredProjects.slice(0, 3));
  renderProjectsTable(filteredProjects);
  renderConversationsTable(filteredConversations);

  // Only render heavy charts if analytics tab is active to avoid lag
  const analyticsTab = document.getElementById('tab-analytics');
  if (analyticsTab && analyticsTab.classList.contains('active')) {
    renderCharts(stats, filteredProjects);
  }
}

// Live Token Metrics Strip
function updateMetricsStrip(currentTotalTokens, stats, filteredProjects) {
  const acc = currentAccountFilter;

  // Session start baseline (safely check for null/undefined)
  if (sessionTokensStartByAccount[acc] == null) {
    sessionTokensStartByAccount[acc] = currentTotalTokens;
    lastKnownTokensByAccount[acc] = currentTotalTokens;
  }

  // Session delta
  const startBaseline = sessionTokensStartByAccount[acc] || currentTotalTokens;
  const sessionDelta = Math.max(0, currentTotalTokens - startBaseline);
  const sessionEl = document.getElementById('live-session-delta');
  if (sessionEl) {
    sessionEl.textContent = sessionDelta > 0 ? `+${formatNumber(sessionDelta)}` : '+0';
  }

  // Step delta (guard against raw lifetime diff)
  const prevTokens = lastKnownTokensByAccount[acc];
  if (prevTokens != null && currentTotalTokens > prevTokens) {
    const rawDelta = currentTotalTokens - prevTokens;
    if (rawDelta > 0 && rawDelta < 2000000) {
      lastStepDeltaByAccount[acc] = rawDelta;

      // Visual glow
      const card = document.getElementById('total-tokens-card');
      if (card) {
        card.classList.remove('token-pulse-glow');
        void card.offsetWidth;
        card.classList.add('token-pulse-glow');
      }
    }
  }
  lastKnownTokensByAccount[acc] = currentTotalTokens;

  // Determine prompt tokens to display
  let promptTokens = lastStepDeltaByAccount[acc] || 0;
  if (!promptTokens && stats && stats.last_prompt_tokens > 0 && stats.last_prompt_tokens < 2000000) {
    promptTokens = stats.last_prompt_tokens;
  }

  const stepEl = document.getElementById('live-step-delta');
  if (stepEl) {
    stepEl.textContent = promptTokens > 0 ? `+${formatNumber(promptTokens)}` : '+0';
    if (stats && stats.last_prompt_in) {
      stepEl.title = `Последний запрос: Входных: ${formatNumber(stats.last_prompt_in)} | Ответ: ${formatNumber(stats.last_prompt_out)}`;
    }
  }

  // Profile indicator in strip
  const profileEl = document.getElementById('live-active-account');
  if (profileEl) {
    if (currentAccountFilter === 'all') {
      profileEl.textContent = 'Все аккаунты';
      profileEl.title = 'Суммарно по всем профилям';
    } else {
      const p = accountsList.find(a => 
        a.id.toLowerCase() === currentAccountFilter.toLowerCase() ||
        (a.email && a.email.toLowerCase() === currentAccountFilter.toLowerCase())
      );
      const name = p ? (p.alias || p.name || p.email) : currentAccountFilter;
      profileEl.textContent = name;
      profileEl.title = p && p.isActive ? 'Активен сейчас в Antigravity' : 'Сохраненные данные профиля';
    }
  }

  // Today tokens
  const todayStr = new Date().toISOString().slice(0, 10);
  let todayTokens = 0;
  if (stats && stats.daily_history) {
    const todayStat = stats.daily_history.find(d => d.date === todayStr);
    if (todayStat) {
      todayTokens = todayStat.total_tokens;
    }
  }
  const todayEl = document.getElementById('live-today-tokens');
  if (todayEl) {
    todayEl.textContent = formatNumber(todayTokens);
  }

  // Separate Gemini vs Claude tokens
  let geminiSpent = 0;
  let claudeSpent = 0;
  for (const p of filteredProjects) {
    geminiSpent += p.gemini_tokens || 0;
    claudeSpent += p.claude_tokens || 0;
  }
  const geminiSpentEl = document.getElementById('gemini-total-spent');
  if (geminiSpentEl) {
    geminiSpentEl.textContent = `Потрачено: ${formatCompactTokens(geminiSpent)} токенов`;
    geminiSpentEl.title = `Всего потрачено Gemini: ${formatNumber(geminiSpent)} токенов`;
  }
  const claudeSpentEl = document.getElementById('claude-total-spent');
  if (claudeSpentEl) {
    claudeSpentEl.textContent = `Потрачено: ${formatCompactTokens(claudeSpent)} токенов`;
    claudeSpentEl.title = `Всего потрачено Claude: ${formatNumber(claudeSpent)} токенов`;
  }

  updateMiniHudUI();
}

// Sync Heartbeat indicator
function triggerSyncPulse() {
  lastSyncTimestamp = Date.now();
  const dot = document.getElementById('sync-dot');
  if (dot) {
    dot.classList.add('pulse');
    setTimeout(() => dot.classList.remove('pulse'), 400);
  }
}

function renderQuickProjects(projects) {
  const container = document.getElementById('quick-projects-list');
  if (!container) return;

  if (!projects || projects.length === 0) {
    container.innerHTML = '<p class="m3-loading">Нет данных</p>';
    return;
  }

  const totalTokens = projects.reduce((acc, p) => acc + (p.total_tokens || 0), 0);

  container.innerHTML = projects.map(p => {
    const pct = totalTokens > 0 ? ((p.total_tokens / totalTokens) * 100).toFixed(1) : '0';
    return `
      <div class="m3-quick-item">
        <div class="m3-qi-left">
          <div class="m3-qi-icon">
            <i data-lucide="folder" style="width: 18px; height: 18px;"></i>
          </div>
          <div>
            <div class="m3-qi-title">${escapeHtml(p.name)}</div>
            <div class="m3-qi-sub">${p.conversation_count} диалогов • ${formatDuration(p.total_duration_seconds)}</div>
          </div>
        </div>
        <div class="m3-qi-right">
          <div class="m3-qi-tokens">${formatNumber(p.total_tokens)}</div>
          <div class="m3-qi-pct">${pct}% от всех</div>
        </div>
      </div>
    `;
  }).join('');

  lucide.createIcons();
}

function getAccountDisplayName(accId) {
  if (!accId) return 'Default';
  const acc = accountsList.find(a => 
    a.id.toLowerCase() === accId.toLowerCase() ||
    (a.email && a.email.toLowerCase() === accId.toLowerCase()) ||
    (a.name && a.name.toLowerCase() === accId.toLowerCase())
  );
  if (acc) {
    return acc.alias || acc.name || acc.email;
  }
  return accId;
}

function renderProjectsTable(projects) {
  const tbody = document.getElementById('projects-tbody');
  if (!tbody) return;

  if (!projects || projects.length === 0) {
    tbody.innerHTML = '<tr><td colspan="9" class="m3-loading">Нет данных</td></tr>';
    return;
  }

  const query = (document.getElementById('project-search')?.value || '').toLowerCase().trim();
  const filtered = query
    ? projects.filter(p => p.name.toLowerCase().includes(query) || p.path.toLowerCase().includes(query))
    : projects;

  const totalTokensAll = projects.reduce((acc, p) => acc + (p.total_tokens || 0), 0);

  tbody.innerHTML = filtered.map(p => {
    const pct = totalTokensAll > 0 ? ((p.total_tokens / totalTokensAll) * 100).toFixed(1) : '0';
    return `
      <tr>
        <td style="font-weight: 700; color: #ffffff;">${escapeHtml(p.name)}</td>
        <td><span class="m3-chip-mini" style="font-size: 0.72rem;">${escapeHtml(getAccountDisplayName(p.account))}</span></td>
        <td><span class="m3-path-text" title="${escapeHtml(p.path)}">${escapeHtml(p.path)}</span></td>
        <td class="serif-table-digit">
          ${formatNumber(p.total_tokens)}
          <span class="table-pct-tag">${pct}%</span>
        </td>
        <td>
          <span style="color: #a8c7fa">${formatNumber(p.total_input_tokens)}</span> / 
          <span style="color: #85d89f">${formatNumber(p.total_output_tokens)}</span>
        </td>
        <td><span class="m3-pill-time">${formatDuration(p.total_duration_seconds)}</span></td>
        <td>${p.conversation_count}</td>
        <td>${formatNumber(p.total_steps)}</td>
        <td style="color: var(--md-sys-color-on-surface-variant); font-size: 0.76rem">${p.last_activity ? p.last_activity.replace('T', ' ').slice(0, 19) : '-'}</td>
      </tr>
    `;
  }).join('');
}

function renderConversationsTable(conversations) {
  const tbody = document.getElementById('conversations-tbody');
  if (!tbody) return;

  if (!conversations || conversations.length === 0) {
    tbody.innerHTML = '<tr><td colspan="8" class="m3-loading">Нет диалогов</td></tr>';
    return;
  }

  tbody.innerHTML = conversations.slice(0, 50).map(c => `
    <tr>
      <td style="font-weight: 600; color: #ffffff;">${escapeHtml(c.title)}</td>
      <td><span class="m3-chip-mini" style="font-size: 0.72rem;">${escapeHtml(getAccountDisplayName(c.account))}</span></td>
      <td><span class="m3-pill-time" style="font-size: 0.76rem">${escapeHtml(c.project_name)}</span></td>
      <td>${c.step_count}</td>
      <td style="color: #a8c7fa; font-family: var(--font-mono)">${formatNumber(c.input_tokens)}</td>
      <td style="color: #85d89f; font-family: var(--font-mono)">${formatNumber(c.output_tokens)}</td>
      <td class="serif-table-digit">${formatNumber(c.total_tokens)}</td>
      <td style="color: var(--md-sys-color-on-surface-variant); font-size: 0.76rem">${c.last_modified ? c.last_modified.replace('T', ' ').slice(0, 16) : '-'}</td>
    </tr>
  `).join('');
}

function renderCharts(stats, filteredProjects) {
  const history = (stats.daily_history || []).slice(-14);
  const labels = history.map(h => h.date);
  const inData = history.map(h => h.input_tokens);
  const outData = history.map(h => h.output_tokens);

  const ctxDaily = document.getElementById('dailyTimelineChart');
  if (ctxDaily) {
    if (dailyChart) dailyChart.destroy();
    dailyChart = new Chart(ctxDaily, {
      type: 'bar',
      data: {
        labels: labels.length ? labels : ['Сегодня'],
        datasets: [
          {
            label: 'Input',
            data: inData.length ? inData : [0],
            backgroundColor: 'rgba(168, 199, 250, 0.85)',
            borderRadius: 8,
          },
          {
            label: 'Output',
            data: outData.length ? outData : [0],
            backgroundColor: 'rgba(133, 216, 159, 0.85)',
            borderRadius: 8,
          }
        ]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            stacked: true,
            grid: { color: 'rgba(255, 255, 255, 0.04)' },
            ticks: { color: '#c4c6d0', font: { family: 'Plus Jakarta Sans', size: 11 } }
          },
          y: {
            stacked: true,
            grid: { color: 'rgba(255, 255, 255, 0.04)' },
            ticks: {
              color: '#c4c6d0',
              font: { family: 'Plus Jakarta Sans', size: 11 },
              callback: v => v >= 1000000 ? (v / 1000000).toFixed(1) + 'M' : v >= 1000 ? (v / 1000).toFixed(0) + 'K' : v
            }
          }
        },
        plugins: {
          legend: {
            labels: { color: '#e2e2e9', font: { family: 'Plus Jakarta Sans', size: 12 } }
          }
        }
      }
    });
  }

  const topProjects = (filteredProjects || []).slice(0, 5);
  const totalProjectsTokens = (filteredProjects || []).reduce((acc, p) => acc + (p.total_tokens || 0), 0);

  const donutTotalEl = document.getElementById('donut-total-tokens');
  if (donutTotalEl) {
    donutTotalEl.textContent = `Всего: ${formatCompactTokens(totalProjectsTokens)} токенов`;
    donutTotalEl.title = `Суммарный расход проектов: ${formatNumber(totalProjectsTokens)} токенов`;
  }

  const projLabels = topProjects.map(p => {
    const pct = totalProjectsTokens > 0 ? Math.round((p.total_tokens / totalProjectsTokens) * 100) : 0;
    return `${p.name} (${pct}%)`;
  });
  const projData = topProjects.map(p => p.total_tokens);

  const ctxProj = document.getElementById('projectsShareChart');
  if (ctxProj) {
    if (projectsChart) projectsChart.destroy();
    projectsChart = new Chart(ctxProj, {
      type: 'doughnut',
      data: {
        labels: projLabels.length ? projLabels : ['-'],
        datasets: [{
          data: projData.length ? projData : [1],
          backgroundColor: [
            '#a8c7fa',
            '#d0bcff',
            '#ffb77c',
            '#85d89f',
            '#e8b9d5',
          ],
          borderColor: '#1e2028',
          borderWidth: 3
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: {
          legend: {
            position: 'bottom',
            labels: { color: '#e2e2e9', font: { family: 'Plus Jakarta Sans', size: 11 }, padding: 14 }
          },
          tooltip: {
            callbacks: {
              label: function(context) {
                const val = context.raw || 0;
                const pct = totalProjectsTokens > 0 ? ((val / totalProjectsTokens) * 100).toFixed(1) : '0';
                return ` ${formatNumber(val)} токенов (${pct}%)`;
              }
            }
          }
        }
      }
    });
  }
}

function escapeHtml(str) {
  if (!str) return '';
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

// Account Switcher Listener
document.querySelectorAll('.m3-acc-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    const acc = btn.getAttribute('data-account');
    if (acc) switchAccount(acc);
  });
});

// Nav listeners
document.querySelectorAll('.m3-nav-pill').forEach(btn => {
  btn.addEventListener('click', () => {
    const tabId = btn.getAttribute('data-tab');
    if (tabId) switchTab(tabId);
  });
});

document.getElementById('project-search')?.addEventListener('input', () => {
  renderProjectsTable(allProjects);
});

document.getElementById('btn-refresh')?.addEventListener('click', async () => {
  const icon = document.getElementById('refresh-icon');
  if (icon) icon.style.transform = 'rotate(360deg)';
  await fetch('/api/refresh', { method: 'POST' });
  await Promise.all([fetchLiveQuota(), fetchStats()]);
  setTimeout(() => {
    if (icon) icon.style.transform = 'none';
  }, 500);
});

// Toast Notification Helper
function showToast(msg, isError = false) {
  let toast = document.getElementById('sentinel-toast');
  if (!toast) {
    toast = document.createElement('div');
    toast.id = 'sentinel-toast';
    toast.style.cssText = `
      position: fixed;
      bottom: 24px;
      right: 24px;
      background: #1e2433;
      border: 1px solid rgba(255,255,255,0.15);
      color: #fff;
      padding: 12px 20px;
      border-radius: 10px;
      font-family: var(--font-sans);
      font-size: 0.88rem;
      font-weight: 600;
      box-shadow: 0 10px 30px rgba(0,0,0,0.5);
      z-index: 9999;
      transition: all 0.25s ease-out;
      opacity: 0;
      transform: translateY(10px);
    `;
    document.body.appendChild(toast);
  }
  toast.textContent = msg;
  toast.style.borderColor = isError ? '#ff6b6b' : '#a8c7fa';
  toast.style.opacity = '1';
  toast.style.transform = 'translateY(0)';
  setTimeout(() => {
    toast.style.opacity = '0';
    toast.style.transform = 'translateY(10px)';
  }, 3500);
}

// Session Work Timer
const sessionStartTime = Date.now();
setInterval(() => {
  const elapsedSecs = Math.floor((Date.now() - sessionStartTime) / 1000);
  const h = Math.floor(elapsedSecs / 3600);
  const m = Math.floor((elapsedSecs % 3600) / 60);
  const s = elapsedSecs % 60;
  const timeStr = `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  const hudTimerEl = document.getElementById('hud-timer-val');
  if (hudTimerEl) hudTimerEl.textContent = timeStr;
}, 1000);

// ==========================================================================
// MINI HUD WIDGET (Always-on-top mode)
// ==========================================================================
let isMiniHudMode = false;
let isHudPinned = true;

async function toggleMiniHud() {
  isMiniHudMode = !isMiniHudMode;
  if (isMiniHudMode) {
    document.body.classList.add('mini-hud-mode');
    const hud = document.getElementById('mini-hud-container');
    if (hud) hud.style.display = 'flex';
    await fetch('/api/window/mini', { method: 'POST' });
    updateMiniHudUI();
    if (window.lucide) lucide.createIcons();
  } else {
    await exitMiniHud();
  }
}

async function exitMiniHud() {
  isMiniHudMode = false;
  document.body.classList.remove('mini-hud-mode');
  const hud = document.getElementById('mini-hud-container');
  if (hud) hud.style.display = 'none';
  await fetch('/api/window/restore', { method: 'POST' });
  if (window.lucide) lucide.createIcons();
}

async function toggleHudPin() {
  isHudPinned = !isHudPinned;
  const btn = document.getElementById('hud-pin-btn');
  if (btn) {
    if (isHudPinned) btn.classList.add('active');
    else btn.classList.remove('active');
  }
  await fetch('/api/window/pin', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ pinned: isHudPinned })
  });
}

function toggleHudSettings() {
  const pop = document.getElementById('hud-settings-popover');
  if (!pop) return;
  pop.style.display = pop.style.display === 'none' ? 'block' : 'none';
}

function saveHudBlockPreferences() {
  const prefs = {
    prompt: document.getElementById('hud-toggle-prompt')?.checked ?? true,
    timer: document.getElementById('hud-toggle-timer')?.checked ?? true,
    gemini: document.getElementById('hud-toggle-gemini')?.checked ?? true,
    claude: document.getElementById('hud-toggle-claude')?.checked ?? true,
    session: document.getElementById('hud-toggle-session')?.checked ?? true,
  };
  localStorage.setItem('sentinel_hud_prefs', JSON.stringify(prefs));
  applyHudBlockPreferences();
}

function applyHudBlockPreferences() {
  let prefs = { prompt: true, timer: true, gemini: true, claude: true, session: true };
  try {
    const saved = localStorage.getItem('sentinel_hud_prefs');
    if (saved) prefs = { ...prefs, ...JSON.parse(saved) };
  } catch (e) {}

  const setVis = (id, checkId, vis) => {
    const el = document.getElementById(id);
    const chk = document.getElementById(checkId);
    if (el) el.style.display = vis ? 'flex' : 'none';
    if (chk) chk.checked = vis;
  };

  setVis('hud-block-prompt', 'hud-toggle-prompt', prefs.prompt);
  setVis('hud-block-timer', 'hud-toggle-timer', prefs.timer);
  setVis('hud-block-gemini', 'hud-toggle-gemini', prefs.gemini);
  setVis('hud-block-claude', 'hud-toggle-claude', prefs.claude);
  setVis('hud-block-session', 'hud-toggle-session', prefs.session);
}

function updateMiniHudUI() {
  // 1. Prompt tokens
  const lastPrompt = rawStats?.last_prompt_tokens || lastStepDelta || 0;
  const promptIn = rawStats?.last_prompt_in || 0;
  const promptOut = rawStats?.last_prompt_out || 0;

  const promptValEl = document.getElementById('hud-prompt-val');
  const promptSplitEl = document.getElementById('hud-prompt-split');
  if (promptValEl) promptValEl.textContent = `+${formatCompactTokens(lastPrompt)}`;
  if (promptSplitEl) promptSplitEl.textContent = `${formatCompactTokens(promptIn)} in • ${formatCompactTokens(promptOut)} out`;

  // 2. Quotas
  if (rawLiveQuota?.quota?.groups) {
    for (const grp of rawLiveQuota.quota.groups) {
      const isGem = grp.displayName.toLowerCase().includes('gemini');
      const b5h = grp.buckets?.find(b => b.window === '5h' || b.bucketId.includes('5h')) || grp.buckets?.[0];
      if (b5h) {
        const pct = Math.round(b5h.remainingFraction * 100);
        const pctEl = document.getElementById(isGem ? 'hud-gemini-pct' : 'hud-claude-pct');
        const barEl = document.getElementById(isGem ? 'hud-gemini-bar' : 'hud-claude-bar');
        const availEl = document.getElementById(isGem ? 'hud-gemini-avail' : 'hud-claude-avail');

        if (pctEl) pctEl.textContent = `${pct}%`;
        if (barEl) barEl.style.width = `${pct}%`;
        if (availEl) {
          const cap = isGem ? 2_000_000 : 750_000;
          availEl.textContent = `~${formatCompactTokens(Math.round(b5h.remainingFraction * cap))}`;
        }
      }
    }
  }

  // 3. Active account
  const actTag = document.getElementById('hud-active-account-tag');
  const actMain = document.getElementById('live-active-account');
  if (actTag && actMain) actTag.textContent = actMain.textContent;

  // 4. Session tokens & value
  const sessionEl = document.getElementById('live-session-delta');
  const hudSessionEl = document.getElementById('hud-session-val');
  const hudSessionCost = document.getElementById('hud-session-cost');
  if (sessionEl && hudSessionEl) {
    hudSessionEl.textContent = sessionEl.textContent;
    const sessTok = parseInt(sessionEl.textContent.replace(/[^0-9]/g, ''), 10) || 0;
    const cost = (sessTok / 1_000_000) * 1.8;
    if (hudSessionCost) hudSessionCost.textContent = `~$${cost.toFixed(2)}`;
  }
}

// ==========================================================================
// DISK & CACHE CLEANER
// ==========================================================================
let cleanerData = null;

function openCleanerModal() {
  const modal = document.getElementById('cleaner-modal');
  if (modal) modal.style.display = 'flex';
  loadCleanerData();
  if (window.lucide) lucide.createIcons();
}

function closeCleanerModal() {
  const modal = document.getElementById('cleaner-modal');
  if (modal) modal.style.display = 'none';
}

async function loadCleanerData() {
  const listEl = document.getElementById('cleaner-items-list');
  const descEl = document.getElementById('cleaner-total-desc');
  if (listEl) listEl.innerHTML = '<div class="cleaner-loading">Сканирование занятого места...</div>';

  try {
    const res = await fetch('/api/cleaner/scan');
    if (!res.ok) return;
    cleanerData = await res.json();

    if (descEl) {
      descEl.textContent = `Общий объем кэша и логов: ${cleanerData.total_formatted_size} (${cleanerData.items.length} источников)`;
    }

    if (listEl) {
      let html = '';
      for (const item of cleanerData.items) {
        const sizeClass = item.size_bytes > 50 * 1024 * 1024 ? 'warning' : '';
        html += `
          <div class="cleaner-row">
            <div class="cleaner-row-left">
              <input type="checkbox" class="cleaner-check" value="${escapeHtml(item.id)}" ${item.size_bytes > 0 ? 'checked' : 'disabled'}>
              <div class="cleaner-row-info">
                <div class="cleaner-row-title">${escapeHtml(item.name)}</div>
                <div class="cleaner-row-desc">${escapeHtml(item.description)}</div>
                <div class="cleaner-row-path" title="${escapeHtml(item.path)}">${escapeHtml(item.path)}</div>
              </div>
            </div>
            <div class="cleaner-row-right">
              <span class="cleaner-size-badge ${sizeClass}">${escapeHtml(item.formatted_size)}</span>
              <button class="cleaner-open-btn" title="Открыть папку в Проводнике" onclick="openCleanerFolder('${escapeHtml(item.path).replace(/\\/g, '\\\\')}')">
                <span>📂</span> Открыть
              </button>
            </div>
          </div>
        `;
      }
      listEl.innerHTML = html;
    }
  } catch (e) {
    if (listEl) listEl.innerHTML = `<div class="cleaner-loading" style="color: #ff6b6b;">Ошибка сканирования: ${e.message}</div>`;
  }
}

async function openCleanerFolder(path) {
  try {
    await fetch('/api/cleaner/open-folder', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path })
    });
  } catch (e) {
    console.error('Failed to open folder:', e);
  }
}

async function submitCleanCaches() {
  const checkboxes = document.querySelectorAll('.cleaner-check:checked');
  const itemIds = Array.from(checkboxes).map(c => c.value);
  if (itemIds.length === 0) {
    alert('Выберите хотя бы один пункт для очистки');
    return;
  }

  const btn = document.getElementById('cleaner-submit-btn');
  if (btn) {
    btn.disabled = true;
    btn.textContent = 'Очистка...';
  }

  try {
    const res = await fetch('/api/cleaner/clean', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ item_ids: itemIds })
    });
    const result = await res.json();
    showToast(`Очищено: освобождено ${result.formatted_freed} дискового пространства! 🧹`);
    await loadCleanerData();
  } catch (e) {
    showToast('Ошибка очистки: ' + e.message, true);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = 'Очистить кэш';
    }
  }
}

// ==========================================================================
// ADVISOR & MODEL EFFICIENCY CALCULATOR
// ==========================================================================
function openAdvisorModal() {
  const modal = document.getElementById('advisor-modal');
  if (modal) modal.style.display = 'flex';

  // Stats calculation
  const geminiTok = rawStats?.gemini_tokens || 0;
  const claudeTok = rawStats?.claude_tokens || 0;
  const totalTok = geminiTok + claudeTok;

  // Official API rates:
  // Gemini 2.5 Pro: ~$1.25 per 1M (avg input $0.35, output $1.05 + caching)
  // Claude 3.5 Sonnet: ~$4.50 per 1M (avg input $3.00, output $15.00)
  const geminiCost = (geminiTok / 1_000_000) * 1.25;
  const claudeCost = (claudeTok / 1_000_000) * 4.50;
  const totalApiCost = geminiCost + claudeCost;

  // Subscription is $20/month
  const savings = Math.max(0, totalApiCost - 20);

  const totalEl = document.getElementById('advisor-tokens-total');
  const splitEl = document.getElementById('advisor-tokens-split');
  const costEl = document.getElementById('advisor-cost-val');
  const saveEl = document.getElementById('advisor-savings-val');

  if (totalEl) totalEl.textContent = formatCompactTokens(totalTok);
  if (splitEl) splitEl.textContent = `Gemini: ${formatCompactTokens(geminiTok)} • Claude: ${formatCompactTokens(claudeTok)}`;
  if (costEl) costEl.textContent = `$${totalApiCost.toFixed(2)}`;
  if (saveEl) saveEl.textContent = `~$${savings.toFixed(2)}`;

  recalcAdvisorTokens();
  if (window.lucide) lucide.createIcons();
}

function closeAdvisorModal() {
  const modal = document.getElementById('advisor-modal');
  if (modal) modal.style.display = 'none';
}

function recalcAdvisorTokens() {
  const input = document.getElementById('calc-million-tokens');
  const container = document.getElementById('calc-table-box');
  if (!input || !container) return;

  const mTokens = parseFloat(input.value) || 1;
  const flashCost = mTokens * 0.10;
  const proCost = mTokens * 1.25;
  const sonnetCost = mTokens * 4.50;

  container.innerHTML = `
    <div class="calc-model-item">
      <div class="calc-m-title">⚡ Gemini Flash</div>
      <div class="calc-m-cost" style="color: #4ade80;">$${flashCost.toFixed(2)}</div>
      <div style="font-size: 0.7rem; color: #64748b; margin-top: 4px;">$0.10 / 1M</div>
    </div>
    <div class="calc-model-item">
      <div class="calc-m-title">🧠 Gemini 2.5 Pro</div>
      <div class="calc-m-cost" style="color: #60a5fa;">$${proCost.toFixed(2)}</div>
      <div style="font-size: 0.7rem; color: #64748b; margin-top: 4px;">$1.25 / 1M</div>
    </div>
    <div class="calc-model-item">
      <div class="calc-m-title">💎 Claude 3.5 Sonnet</div>
      <div class="calc-m-cost" style="color: #fb923c;">$${sonnetCost.toFixed(2)}</div>
      <div style="font-size: 0.7rem; color: #64748b; margin-top: 4px;">$4.50 / 1M</div>
    </div>
  `;
}

// ==========================================================================
// AUTO-UPDATER & HOT SWAP
// ==========================================================================
let latestUpdateData = null;

async function checkForUpdates(silent = true) {
  try {
    const res = await fetch('/api/update/check');
    if (!res.ok) {
      if (!silent) showToast('Не удалось связаться с сервером обновлений', true);
      return;
    }
    const data = await res.json();
    latestUpdateData = data;

    const versionBtn = document.getElementById('version-text');
    if (versionBtn) versionBtn.textContent = `v${data.current_version}`;

    const updateAlertBtn = document.getElementById('btn-update-alert');
    const updateBadgeText = document.getElementById('update-badge-text');

    if (data.has_update) {
      if (updateAlertBtn) updateAlertBtn.style.display = 'inline-flex';
      if (updateBadgeText) updateBadgeText.textContent = `⚡ v${data.latest_version}`;
      if (!silent) {
        openUpdateModal();
      }
    } else {
      if (updateAlertBtn) updateAlertBtn.style.display = 'none';
      if (!silent) {
        showToast(`У вас актуальная версия (${data.current_version}) 👍`);
      }
    }
  } catch (e) {
    if (!silent) showToast('Ошибка проверки обновлений: ' + e.message, true);
  }
}

function checkUpdateManual() {
  showToast('Проверка обновлений на GitHub...');
  checkForUpdates(false);
}

function openUpdateModal() {
  if (!latestUpdateData) return;
  const modal = document.getElementById('update-modal');
  const label = document.getElementById('update-version-label');
  const notes = document.getElementById('update-notes-content');
  const pBox = document.getElementById('update-progress-box');
  const btn = document.getElementById('btn-do-update');

  if (label) label.textContent = `Доступна: v${latestUpdateData.latest_version} • У вас: v${latestUpdateData.current_version}`;
  if (notes) notes.textContent = latestUpdateData.release_notes || 'Список изменений не указан.';
  if (pBox) pBox.style.display = 'none';
  if (btn) {
    btn.disabled = false;
    btn.innerHTML = '<i data-lucide="download" style="width: 14px; height: 14px; margin-right: 6px;"></i> Обновить и перезапустить';
  }

  if (modal) modal.style.display = 'flex';
  if (window.lucide) lucide.createIcons();
}

function closeUpdateModal() {
  const modal = document.getElementById('update-modal');
  if (modal) modal.style.display = 'none';
}

async function installUpdateNow() {
  if (!latestUpdateData || !latestUpdateData.download_url) {
    showToast('Ссылка на бинарник релиза не найдена на GitHub', true);
    return;
  }

  const btn = document.getElementById('btn-do-update');
  const pBox = document.getElementById('update-progress-box');
  const statusText = document.getElementById('update-status-text');

  if (btn) btn.disabled = true;
  if (pBox) pBox.style.display = 'block';
  if (statusText) statusText.textContent = 'Скачивание новой версии и горячая замена...';

  try {
    const res = await fetch('/api/update/install', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ download_url: latestUpdateData.download_url })
    });

    if (res.ok) {
      if (statusText) statusText.textContent = 'Перезапуск программы в обновленной версии...';
      showToast('Обновление завершено! Перезапуск... 🚀');
    } else {
      const err = await res.text();
      showToast('Ошибка обновления: ' + err, true);
      if (btn) btn.disabled = false;
    }
  } catch (e) {
    showToast('Ошибка: ' + e.message, true);
    if (btn) btn.disabled = false;
  }
}

// Bootstrapping
document.addEventListener('DOMContentLoaded', async () => {
  lucide.createIcons();
  updateSoundAlertUI();
  applyHudBlockPreferences();
  await fetchAccounts();
  fetchLiveQuota();
  fetchStats();
  setTimeout(() => checkForUpdates(true), 1500);

  setInterval(fetchLiveQuota, 3000);
  setInterval(fetchStats, 10000);
  setInterval(updateMiniHudUI, 3000);
  setInterval(() => checkForUpdates(true), 1000 * 60 * 60); // check hourly
});
