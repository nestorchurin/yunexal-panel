// Schedules page
const SID = window.YU_SERVER_ID;
let _editingId = null;
let _modal = null;

window._yuPageCleanup = function () {};

function _schedOnPageShown(path) {
    if (!path.includes('/schedules')) return;
    if (_modal) { try { _modal.dispose(); } catch {} _modal = null; }
    const el = document.getElementById('schedModal');
    if (el) _modal = new bootstrap.Modal(el);
    loadSchedules();
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => _schedOnPageShown(window.location.pathname), { once: true });
} else {
    _schedOnPageShown(window.location.pathname);
}

window.addEventListener('yu:page-shown', (ev) => {
    _schedOnPageShown(String(ev?.detail?.path || window.location.pathname));
});

// ── Data loading ────────────────────────────────────────────────────────────

async function loadSchedules() {
    try {
        const r = await fetch(`/api/servers/${SID}/schedules`);
        const d = await r.json();
        if (!r.ok) { renderError(d.error || 'Failed to load'); return; }
        renderSchedules(d.schedules || []);
    } catch (e) {
        renderError('Network error');
    }
}

function renderError(msg) {
    document.getElementById('sched-list').innerHTML =
        `<div class="yu-panel" style="padding:2rem;text-align:center;color:var(--err);font-size:.85rem;">${msg}</div>`;
}

function renderSchedules(list) {
    const el = document.getElementById('sched-list');
    if (!list.length) {
        el.innerHTML = `
        <div class="yu-panel" style="padding:2.5rem;text-align:center;">
            <i class="bi bi-calendar-x" style="font-size:2rem;color:var(--muted);opacity:.4;"></i>
            <p style="margin:.75rem 0 0;font-size:.875rem;color:var(--muted);">No schedules yet. Create one to get started.</p>
        </div>`;
        return;
    }

    el.innerHTML = list.map(s => `
    <div class="yu-panel" id="sched-${s.id}" style="margin-bottom:.65rem;">
        <div style="padding:.75rem 1rem;display:flex;align-items:center;gap:.75rem;flex-wrap:wrap;">
            <div style="flex:1;min-width:0;">
                <div style="display:flex;align-items:center;gap:.5rem;flex-wrap:wrap;margin-bottom:.25rem;">
                    <span style="font-size:.875rem;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">${escHtml(s.name)}</span>
                    ${typeBadge(s.task_type)}
                    ${s.enabled ? '' : '<span style="font-size:.68rem;font-weight:600;background:rgba(107,114,128,.15);color:#9ca3af;border:1px solid rgba(107,114,128,.25);border-radius:5px;padding:.15rem .45rem;letter-spacing:.04em;">DISABLED</span>'}
                </div>
                <div style="display:flex;gap:1rem;flex-wrap:wrap;font-size:.75rem;color:var(--muted);">
                    <span title="Cron expression"><i class="bi bi-clock" style="margin-right:.25rem;"></i><code style="font-size:.73rem;">${escHtml(s.cron_expression)}</code></span>
                    <span title="Next run"><i class="bi bi-arrow-right-circle" style="margin-right:.25rem;"></i>${s.next_run_at ? fmtTime(s.next_run_at) : '—'}</span>
                    <span title="Last run"><i class="bi bi-check2-circle" style="margin-right:.25rem;"></i>${s.last_run_at ? fmtTime(s.last_run_at) : 'Never'}</span>
                </div>
            </div>
            <div style="display:flex;align-items:center;gap:.4rem;flex-shrink:0;">
                <button class="btn-yu btn-yu-ghost" style="padding:.3rem .55rem;font-size:.75rem;" onclick="runNow(${s.id})" title="Run now">
                    <i class="bi bi-play-fill"></i>
                </button>
                <button class="btn-yu btn-yu-ghost" style="padding:.3rem .55rem;font-size:.75rem;" onclick="loadRuns(${s.id}, this)" title="Run history">
                    <i class="bi bi-list-ul"></i>
                </button>
                <button class="btn-yu btn-yu-ghost" style="padding:.3rem .55rem;font-size:.75rem;" onclick="toggleEnabled(${s.id}, ${s.enabled}, this)" title="${s.enabled ? 'Disable' : 'Enable'}">
                    <i class="bi bi-${s.enabled ? 'pause-fill' : 'play-circle'}"></i>
                </button>
                <button class="btn-yu btn-yu-ghost" style="padding:.3rem .55rem;font-size:.75rem;" onclick="openModal(${JSON.stringify(s).replace(/"/g,'&quot;')})" title="Edit">
                    <i class="bi bi-pencil"></i>
                </button>
                <button class="btn-yu btn-yu-ghost" style="padding:.3rem .55rem;font-size:.75rem;color:var(--err);" onclick="deleteSchedule(${s.id}, '${escHtml(s.name)}')" title="Delete">
                    <i class="bi bi-trash3"></i>
                </button>
            </div>
        </div>
        <div id="runs-${s.id}" style="display:none;border-top:1px solid var(--bdr);padding:.6rem 1rem;font-size:.78rem;color:var(--muted);">
            <i class="bi bi-hourglass-split" style="margin-right:.3rem;"></i>Loading…
        </div>
    </div>`).join('');
}

function typeBadge(type) {
    const cfg = {
        command: { bg: 'rgba(99,102,241,.12)', color: '#818cf8', border: 'rgba(99,102,241,.2)', icon: 'bi-terminal' },
        power:   { bg: 'rgba(245,158,11,.1)',  color: '#f59e0b', border: 'rgba(245,158,11,.2)', icon: 'bi-lightning' },
        backup:  { bg: 'rgba(16,185,129,.1)',  color: '#10b981', border: 'rgba(16,185,129,.2)', icon: 'bi-archive'  },
    };
    const c = cfg[type] || cfg.command;
    return `<span style="font-size:.68rem;font-weight:600;background:${c.bg};color:${c.color};border:1px solid ${c.border};border-radius:5px;padding:.15rem .45rem;display:inline-flex;align-items:center;gap:.25rem;letter-spacing:.03em;"><i class="bi ${c.icon}"></i> ${type}</span>`;
}

function fmtTime(ts) {
    if (!ts) return '—';
    try {
        const d = new Date(ts.replace(' ', 'T') + 'Z');
        return d.toLocaleString([], { dateStyle: 'short', timeStyle: 'short' });
    } catch { return ts; }
}

// ── Modal ────────────────────────────────────────────────────────────────────

function openModal(sched) {
    _editingId = sched ? sched.id : null;
    document.getElementById('modal-title').innerHTML =
        `<i class="bi bi-calendar-event me-2" style="color:#a78bfa;"></i>${sched ? 'Edit Schedule' : 'New Schedule'}`;
    document.getElementById('m-name').value    = sched ? sched.name : '';
    document.getElementById('m-cron').value    = sched ? sched.cron_expression : '0 3 * * *';
    document.getElementById('m-type').value    = sched ? sched.task_type : 'command';
    document.getElementById('m-enabled').checked = sched ? sched.enabled : true;

    if (sched) {
        try {
            const p = JSON.parse(sched.task_payload || '{}');
            document.getElementById('m-cmd').value    = p.cmd    || '';
            document.getElementById('m-action').value = p.action || 'restart';
            document.getElementById('m-prefix').value = p.prefix || 'backup';
        } catch { }
    } else {
        document.getElementById('m-cmd').value    = '';
        document.getElementById('m-action').value = 'restart';
        document.getElementById('m-prefix').value = 'backup';
    }

    onTypeChange();
    updateCronPreview();
    _modal.show();
}

function onTypeChange() {
    const t = document.getElementById('m-type').value;
    document.getElementById('payload-command').style.display = t === 'command' ? '' : 'none';
    document.getElementById('payload-power').style.display   = t === 'power'   ? '' : 'none';
    document.getElementById('payload-backup').style.display  = t === 'backup'  ? '' : 'none';
}

function updateCronPreview() {
    const expr = document.getElementById('m-cron').value.trim();
    document.getElementById('m-cron-preview').textContent = describeCron(expr);
}

function describeCron(expr) {
    if (!expr) return '';
    const parts = expr.trim().split(/\s+/);
    if (parts.length !== 5) return 'Invalid — must be 5 fields (min hour dom month dow)';
    const [min, hr, dom, mon, dow] = parts;

    if (expr === '* * * * *') return 'Every minute';
    if (min.startsWith('*/') && hr === '*' && dom === '*' && mon === '*' && dow === '*') {
        const n = parseInt(min.slice(2));
        return `Every ${n} minute${n !== 1 ? 's' : ''}`;
    }
    if (min === '0' && hr.startsWith('*/') && dom === '*' && mon === '*' && dow === '*') {
        const n = parseInt(hr.slice(2));
        return `Every ${n} hour${n !== 1 ? 's' : ''}`;
    }
    if (dom === '*' && mon === '*' && dow === '*') {
        if (/^\d+$/.test(min) && /^\d+$/.test(hr)) {
            const h = parseInt(hr), m = parseInt(min);
            return `Daily at ${String(h).padStart(2,'0')}:${String(m).padStart(2,'0')}`;
        }
    }
    if (dom === '*' && mon === '*' && /^\d+$/.test(dow)) {
        const days = ['Sunday','Monday','Tuesday','Wednesday','Thursday','Friday','Saturday'];
        const d = days[parseInt(dow)] || `dow ${dow}`;
        if (/^\d+$/.test(min) && /^\d+$/.test(hr)) {
            const h = parseInt(hr), m = parseInt(min);
            return `Every ${d} at ${String(h).padStart(2,'0')}:${String(m).padStart(2,'0')}`;
        }
    }
    return 'Custom cron expression';
}

async function saveSchedule() {
    const name = document.getElementById('m-name').value.trim();
    const cron = document.getElementById('m-cron').value.trim();
    const type = document.getElementById('m-type').value;
    const enabled = document.getElementById('m-enabled').checked;

    if (!name) { showToast('danger', 'Name is required'); return; }
    if (!cron)  { showToast('danger', 'Cron expression is required'); return; }

    let payload = {};
    if (type === 'command') payload = { cmd: document.getElementById('m-cmd').value.trim() };
    if (type === 'power')   payload = { action: document.getElementById('m-action').value };
    if (type === 'backup')  payload = { prefix: document.getElementById('m-prefix').value.trim() || 'backup' };

    const btn = document.getElementById('m-save-btn');
    const orig = btn.innerHTML;
    btn.disabled = true;
    btn.innerHTML = '<i class="bi bi-hourglass-split"></i> Saving…';

    try {
        const url = _editingId
            ? `/api/servers/${SID}/schedules/${_editingId}/update`
            : `/api/servers/${SID}/schedules`;
        const body = { name, cron_expression: cron, task_type: type, task_payload: JSON.stringify(payload), enabled };
        const r = await fetch(url, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
        });
        const d = await r.json();
        if (!r.ok) { showToast('danger', d.error || 'Failed to save'); }
        else {
            _modal.hide();
            showToast('success', _editingId ? 'Schedule updated' : 'Schedule created');
            loadSchedules();
        }
    } catch { showToast('danger', 'Network error'); }

    btn.disabled = false;
    btn.innerHTML = orig;
}

// ── Actions ──────────────────────────────────────────────────────────────────

async function toggleEnabled(id, currentlyEnabled, btn) {
    const newVal = !currentlyEnabled;
    const origHtml = btn.innerHTML;
    btn.disabled = true;
    try {
        const r = await fetch(`/api/servers/${SID}/schedules/${id}/toggle`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ enabled: newVal }),
        });
        const d = await r.json();
        if (r.ok) {
            showToast('success', newVal ? 'Schedule enabled' : 'Schedule disabled');
            loadSchedules();
        } else {
            showToast('danger', d.error || 'Failed');
            btn.disabled = false;
            btn.innerHTML = origHtml;
        }
    } catch {
        showToast('danger', 'Network error');
        btn.disabled = false;
        btn.innerHTML = origHtml;
    }
}

async function deleteSchedule(id, name) {
    const ok = await window.yuConfirm(`Delete schedule "${name}"?`, {
        icon: 'bi-trash3-fill',
        iconColor: '#f87171',
        subtitle: 'All run history will also be deleted.',
        okLabel: 'Delete',
    });
    if (!ok) return;
    try {
        const r = await fetch(`/api/servers/${SID}/schedules/${id}/delete`, { method: 'POST' });
        const d = await r.json();
        if (r.ok) { showToast('success', 'Schedule deleted'); loadSchedules(); }
        else showToast('danger', d.error || 'Failed to delete');
    } catch { showToast('danger', 'Network error'); }
}

async function runNow(id) {
    try {
        const r = await fetch(`/api/servers/${SID}/schedules/${id}/run-now`, { method: 'POST' });
        const d = await r.json();
        if (r.ok) showToast('success', 'Queued — check run history shortly');
        else showToast('danger', d.error || 'Failed');
    } catch { showToast('danger', 'Network error'); }
}

async function loadRuns(id, btn) {
    const runsEl = document.getElementById(`runs-${id}`);
    if (runsEl.style.display !== 'none') {
        runsEl.style.display = 'none';
        return;
    }
    runsEl.style.display = '';
    runsEl.innerHTML = '<i class="bi bi-hourglass-split" style="margin-right:.3rem;"></i>Loading…';
    try {
        const r = await fetch(`/api/servers/${SID}/schedules/${id}/runs`);
        const d = await r.json();
        if (!r.ok) { runsEl.innerHTML = `<span style="color:var(--err);">${d.error || 'Failed'}</span>`; return; }
        const runs = d.runs || [];
        if (!runs.length) { runsEl.innerHTML = '<span style="color:var(--muted);">No runs yet.</span>'; return; }
        const statusColor = { success: '#10b981', error: '#ef4444', running: '#f59e0b' };
        runsEl.innerHTML = `
        <div style="font-size:.72rem;font-weight:600;color:var(--muted);text-transform:uppercase;letter-spacing:.06em;margin-bottom:.45rem;">Run History</div>
        <div style="display:flex;flex-direction:column;gap:.35rem;">
            ${runs.map(r => `
            <div style="display:flex;gap:.75rem;align-items:flex-start;padding:.35rem .5rem;border-radius:7px;background:rgba(255,255,255,.02);border:1px solid rgba(255,255,255,.04);">
                <span style="font-size:.72rem;color:${statusColor[r.status]||'#9ca3af'};font-weight:600;min-width:52px;flex-shrink:0;">${r.status.toUpperCase()}</span>
                <span style="font-size:.73rem;color:var(--muted);white-space:nowrap;flex-shrink:0;">${fmtTime(r.started_at)}</span>
                ${r.output ? `<span style="font-size:.73rem;color:var(--txt);font-family:monospace;word-break:break-all;">${escHtml(r.output)}</span>` : ''}
            </div>`).join('')}
        </div>`;
    } catch { runsEl.innerHTML = '<span style="color:var(--err);">Network error</span>'; }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function escHtml(s) {
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

function showToast(type, msg) {
    let tc = document.getElementById('st-toast-container');
    if (!tc) {
        tc = document.createElement('div');
        tc.id = 'st-toast-container';
        tc.style.cssText = 'position:fixed;bottom:1.25rem;right:1.25rem;z-index:99999;display:flex;flex-direction:column;gap:.45rem;pointer-events:none;';
        document.body.appendChild(tc);
    }
    const bg  = {success:'rgba(34,197,94,.14)', danger:'rgba(239,68,68,.14)', warning:'rgba(251,191,36,.14)'};
    const bd  = {success:'rgba(34,197,94,.3)',  danger:'rgba(239,68,68,.3)',  warning:'rgba(251,191,36,.3)'};
    const tx  = {success:'#86efac',            danger:'#fca5a5',             warning:'#fde68a'};
    const el  = document.createElement('div');
    el.style.cssText = `background:${bg[type]||bg.danger};border:1px solid ${bd[type]||bd.danger};color:${tx[type]||tx.danger};padding:.55rem 1rem;border-radius:8px;font-size:.825rem;font-weight:500;backdrop-filter:blur(8px);opacity:0;transform:translateX(20px);transition:all .25s;white-space:nowrap;`;
    el.textContent = msg;
    tc.appendChild(el);
    requestAnimationFrame(() => requestAnimationFrame(() => { el.style.opacity = '1'; el.style.transform = 'none'; }));
    setTimeout(() => { el.style.opacity = '0'; el.style.transform = 'translateX(20px)'; setTimeout(() => el.remove(), 280); }, 3400);
}
