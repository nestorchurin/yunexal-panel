// Dashboard: in-place JSON polling (no animation replay) + live stats + rename modal

let _editServerId = null;
let _isAdmin      = false;

// ── Helpers ────────────────────────────────────────────────────────────────
function esc(s) {
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;')
        .replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
}

function _stripeClass(state) {
    return state === 'running' ? 'stripe-running' : state === 'restarting' ? 'stripe-other' : 'stripe-stopped';
}
function _pillClass(state) {
    return state === 'running' ? 'pill-running' : state === 'restarting' ? 'pill-other' : 'pill-stopped';
}

// ── Build a brand-new card (only called for newly appeared servers) ────────
function buildCard(c) {
    const running = c.state === 'running';
    const del = _isAdmin
        ? `<button class="btn-yu btn-yu-danger btn-yu-icon" onclick="dashDelete(${c.db_id},'${esc(c.name)}')" title="Delete"><i class="bi bi-trash"></i></button>`
        : '';
    const wrap = document.createElement('div');
    wrap.innerHTML = `<div class="col-md-6 col-xl-4"
        id="container-${c.db_id}"
        data-dash-id="${c.db_id}"
        data-state="${esc(c.state)}">
  <div class="yu-card h-100">
    <div class="card-stripe ${_stripeClass(c.state)}" data-el="stripe"></div>
    <div class="p-3">
      <div class="d-flex align-items-start justify-content-between mb-2">
        <div style="font-size:.95rem;font-weight:600;letter-spacing:-.015em;">${esc(c.name)}</div>
        <div class="status-pill ${_pillClass(c.state)}" data-el="pill">
          <span class="pill-dot"></span>
          <span data-el="status">${esc(c.status)}</span>
        </div>
      </div>
      <div class="mb-3" style="font-size:.7rem;font-family:monospace;color:var(--muted);background:var(--surface2);padding:.18rem .5rem;border-radius:5px;display:inline-block;">#${c.db_id}</div>
      <div class="row g-2 mb-3">
        <div class="col-6"><div class="stat-box"><div class="stat-lbl">CPU</div><div class="stat-val" id="dash-cpu-${c.db_id}">—</div></div></div>
        <div class="col-6"><div class="stat-box"><div class="stat-lbl">RAM</div><div class="stat-val" id="dash-ram-${c.db_id}">—</div></div></div>
      </div>
      <div class="d-flex gap-2 flex-wrap align-items-center">
        <a href="/servers/${c.db_id}/console" class="btn-yu btn-yu-primary" style="flex:1;justify-content:center;">
          <i class="bi bi-terminal-fill"></i> Console
        </a>
        <button data-el="action-btn"
                class="btn-yu ${running ? 'btn-yu-danger' : 'btn-yu-success'}"
                onclick="dashAction(${c.db_id},'${running ? 'stop' : 'start'}')">
          <i class="bi ${running ? 'bi-stop-fill' : 'bi-play-fill'}"></i> ${running ? 'Stop' : 'Start'}
        </button>
        <button class="btn-yu btn-yu-ghost btn-yu-icon"
                onclick="openEditModal('${c.db_id}','${esc(c.name)}')"
                data-bs-toggle="modal" data-bs-target="#editServerModal" title="Edit">
          <i class="bi bi-pencil"></i>
        </button>
        ${del}
      </div>
    </div>
  </div>
</div>`;
    return wrap.firstElementChild;
}

// ── Update existing card in-place — NO DOM recreation, NO animation replay ──
function updateCardInPlace(card, c) {
    const running = c.state === 'running';
    card.dataset.state = c.state;

    const stripe = card.querySelector('[data-el="stripe"]');
    if (stripe) {
        const cls = 'card-stripe ' + _stripeClass(c.state);
        if (stripe.className !== cls) stripe.className = cls;
    }

    const pill = card.querySelector('[data-el="pill"]');
    if (pill) {
        const cls = 'status-pill ' + _pillClass(c.state);
        if (pill.className !== cls) pill.className = cls;
    }

    const statusEl = card.querySelector('[data-el="status"]');
    if (statusEl && statusEl.textContent !== c.status) statusEl.textContent = c.status;

    const btn = card.querySelector('[data-el="action-btn"]');
    if (btn) {
        btn.disabled = false;
        const wasRunning = btn.classList.contains('btn-yu-danger');
        if (wasRunning !== running || btn.querySelector('.spinner-border')) {
            btn.className = 'btn-yu ' + (running ? 'btn-yu-danger' : 'btn-yu-success');
            btn.setAttribute('onclick', `dashAction(${c.db_id},'${running ? 'stop' : 'start'}')`);
            btn.innerHTML = `<i class="bi ${running ? 'bi-stop-fill' : 'bi-play-fill'}"></i> ${running ? 'Stop' : 'Start'}`;
        }
    }

    if (!running) {
        const cpu = document.getElementById('dash-cpu-' + c.db_id);
        const ram = document.getElementById('dash-ram-' + c.db_id);
        if (cpu && cpu.textContent !== '—') cpu.textContent = '—';
        if (ram && ram.textContent !== '—') ram.textContent = '—';
    }
}

// ── List polling (5s) ─────────────────────────────────────────────────────
function loadServers() {
    if (document.querySelector('.modal.show')) return;
    fetch('/api/dashboard', { credentials: 'same-origin' })
        .then(r => {
            if (r.status === 401 || r.redirected) { window.location.href = '/login'; return null; }
            return r.json();
        })
        .then(data => {
            if (!data || !data.ok) return;
            _isAdmin = !!data.is_admin;
            const list = document.getElementById('server-list');
            if (!list) return;

            const seen = new Set();
            data.containers.forEach(c => {
                seen.add(String(c.db_id));
                const existing = document.getElementById('container-' + c.db_id);
                if (existing) {
                    updateCardInPlace(existing, c);
                } else {
                    const empty = list.querySelector('[data-empty]');
                    if (empty) empty.remove();
                    list.appendChild(buildCard(c));
                }
            });

            // Remove cards for deleted servers
            list.querySelectorAll('[data-dash-id]').forEach(card => {
                if (!seen.has(card.dataset.dashId)) card.remove();
            });

            // Empty state
            if (data.containers.length === 0 && !list.querySelector('[data-dash-id]') && !list.querySelector('[data-empty]')) {
                const newBtn = _isAdmin ? `<a href="/servers/new" class="btn-yu btn-yu-primary"><i class="bi bi-plus-lg"></i> New Server</a>` : '';
                const el = document.createElement('div');
                el.dataset.empty = '1';
                el.className = 'col-12';
                el.innerHTML = `<div style="display:flex;flex-direction:column;align-items:center;justify-content:center;padding:4.5rem 2rem;border:1px dashed rgba(255,255,255,.1);border-radius:14px;background:rgba(255,255,255,.02);text-align:center;">
  <div style="width:56px;height:56px;border-radius:14px;background:rgba(124,58,237,.1);border:1px solid rgba(124,58,237,.2);display:flex;align-items:center;justify-content:center;margin-bottom:1.25rem;"><i class="bi bi-server" style="font-size:1.4rem;color:#a78bfa;"></i></div>
  <div style="font-size:1rem;font-weight:600;letter-spacing:-.015em;margin-bottom:.4rem;">No servers yet</div>
  <p style="font-size:.825rem;color:var(--muted);max-width:300px;margin-bottom:1.5rem;">Create your first server to get started.</p>
  ${newBtn}</div>`;
                list.appendChild(el);
            }
        })
        .catch(() => {});
}

// ── Stats polling (1s, only running cards) ────────────────────────────────
function pollStats() {
    document.querySelectorAll('[data-dash-id][data-state="running"]').forEach(card => {
        const id = card.dataset.dashId;
        fetch(`/api/servers/${id}/stats`)
            .then(r => r.json())
            .then(s => {
                const cpu = document.getElementById('dash-cpu-' + id);
                const ram = document.getElementById('dash-ram-' + id);
                if (cpu) cpu.textContent = s.cpu !== undefined ? s.cpu.toFixed(2) + '%' : '—';
                if (ram) ram.textContent = s.ram !== undefined
                    ? `${(s.ram / 1048576).toFixed(0)}MB / ${(s.ram_limit / 1048576).toFixed(0)}MB`
                    : '—';
            })
            .catch(() => {});
    });
}

// ── Visibility-aware polling (mobile: timers freeze when tab is background) ──
let _listTimer  = null;
let _statsTimer = null;

function _startTimers() {
    if (_listTimer)  clearInterval(_listTimer);
    if (_statsTimer) clearInterval(_statsTimer);
    _listTimer  = setInterval(loadServers, 5000);
    _statsTimer = setInterval(pollStats,   1000);
}

// On mobile, when the user returns to the tab — refresh immediately then restart timers
document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') {
        loadServers();
        pollStats();
        _startTimers();
    } else {
        clearInterval(_listTimer);
        clearInterval(_statsTimer);
    }
});

_startTimers();

// ── Actions ───────────────────────────────────────────────────────────────
function dashAction(id, action) {
    const card = document.getElementById('container-' + id);
    const btn  = card && card.querySelector('[data-el="action-btn"]');
    if (btn) {
        btn.disabled = true;
        btn.innerHTML = '<span class="spinner-border spinner-border-sm"></span>';
    }
    // Optimistic status text — instant mobile feedback without waiting for server
    if (card) {
        const statusEl = card.querySelector('[data-el="status"]');
        if (statusEl) statusEl.textContent = action === 'stop' ? 'Stopping…' : 'Starting…';
    }
    fetch(`/api/servers/${id}/${action}`, { method: 'POST', credentials: 'same-origin' })
        .finally(() => loadServers());
}

// ── Fullscreen mode ────────────────────────────────────────────────────────
function _updateFsBtn() {
    const btn = document.getElementById('fs-btn');
    if (!btn) return;
    const isFs = !!(document.fullscreenElement || document.webkitFullscreenElement);
    btn.querySelector('i').className = isFs ? 'bi bi-fullscreen-exit' : 'bi bi-fullscreen';
    btn.title = isFs ? 'Вийти з повноекранного' : 'Повний екран';
}
function toggleFullscreen() {
    if (document.fullscreenElement || document.webkitFullscreenElement) {
        (document.exitFullscreen || document.webkitExitFullscreen).call(document);
    } else {
        const el = document.documentElement;
        (el.requestFullscreen || el.webkitRequestFullscreen).call(el);
    }
}
document.addEventListener('fullscreenchange',       _updateFsBtn);
document.addEventListener('webkitfullscreenchange', _updateFsBtn);

// ── AMOLED mode ─────────────────────────────────────────────────────────────
function _applyAmoled(on) {
    document.body.classList.toggle('amoled', on);
    const btn = document.getElementById('amoled-btn');
    if (btn) btn.title = on ? 'Вимкнути AMOLED режим' : 'AMOLED режим';
}
function toggleAmoled() {
    const on = !document.body.classList.contains('amoled');
    localStorage.setItem('amoled', on ? '1' : '0');
    _applyAmoled(on);
    if (on) {
        const el = document.documentElement;
        (el.requestFullscreen || el.webkitRequestFullscreen || function(){}).call(el);
    } else {
        if (document.fullscreenElement || document.webkitFullscreenElement) {
            (document.exitFullscreen || document.webkitExitFullscreen).call(document);
        }
    }
}
// Apply saved preference immediately (before first paint)
_applyAmoled(localStorage.getItem('amoled') === '1');

async function dashDelete(id, name) {
    if (!await yuConfirm(`Delete server "${name}"?`)) return;
    const card = document.getElementById('container-' + id);
    if (card) card.style.opacity = '0.4';
    fetch(`/api/servers/${id}/delete`, { method: 'POST' })
        .catch(() => { if (card) card.style.opacity = ''; });
}

// ── Rename modal ──────────────────────────────────────────────────────────
function openEditModal(id, name) {
    _editServerId = id;
    document.getElementById('editServerName').value = name;
}

document.addEventListener('DOMContentLoaded', () => {
    const form = document.getElementById('editServerForm');
    if (!form) return;
    form.addEventListener('submit', async function (e) {
        e.preventDefault();
        if (!_editServerId) return;
        const name = document.getElementById('editServerName').value.trim();
        if (!name) return;
        const res = await fetch(`/api/servers/${_editServerId}/rename`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            body: new URLSearchParams({ name })
        });
        bootstrap.Modal.getInstance(document.getElementById('editServerModal')).hide();
        if (!res.ok) alert('Rename failed: ' + await res.text());
    });
});
