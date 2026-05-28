// Schedules page
const SID = window.YU_SERVER_ID;
let _editingId = null;
let _modal = null;
const _scriptStepsEl = () => document.getElementById('m-script-steps');

const _SCRIPT_STEP_INFO = {
    command: {
        label: 'Command',
        hint: 'Runs a shell command inside the container.',
    },
    power: {
        label: 'Power',
        hint: 'Starts, stops, restarts, or kills the container.',
    },
    backup: {
        label: 'Backup',
        hint: 'Creates a volume archive with the selected prefix.',
    },
    delay: {
        label: 'Delay',
        hint: 'Waits before the next step starts.',
    },
};

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
                ${s.task_type === 'script' ? `<div style="margin-top:.45rem;"><span class="con-sched-summary"><i class="bi bi-diagram-3"></i>${escHtml(scriptSummary(s.task_payload))}</span></div>` : ''}
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
        command: { bg: 'rgba(99,102,241,.12)', color: '#818cf8', border: 'rgba(99,102,241,.2)', icon: 'bi-terminal', label: 'Command' },
        power:   { bg: 'rgba(245,158,11,.1)',  color: '#f59e0b', border: 'rgba(245,158,11,.2)', icon: 'bi-lightning', label: 'Power' },
        backup:  { bg: 'rgba(16,185,129,.1)',  color: '#10b981', border: 'rgba(16,185,129,.2)', icon: 'bi-archive', label: 'Backup'  },
        script:  { bg: 'rgba(124,58,237,.12)', color: '#c4b5fd', border: 'rgba(124,58,237,.22)', icon: 'bi-diagram-3', label: 'Scenario' },
    };
    const c = cfg[type] || cfg.command;
    return `<span style="font-size:.68rem;font-weight:600;background:${c.bg};color:${c.color};border:1px solid ${c.border};border-radius:5px;padding:.15rem .45rem;display:inline-flex;align-items:center;gap:.25rem;letter-spacing:.03em;"><i class="bi ${c.icon}"></i> ${c.label}</span>`;
}

function scriptSummary(taskPayload) {
    try {
        const payload = JSON.parse(taskPayload || '{}');
        const steps = Array.isArray(payload.steps) ? payload.steps : [];
        if (!steps.length) return 'Empty scenario';

        const preview = steps.slice(0, 3).map((step) => {
            const type = String(step?.type || '').toLowerCase();
            if (type === 'command') return 'command';
            if (type === 'power') return `power:${String(step.action || 'restart')}`;
            if (type === 'backup') return 'backup';
            if (type === 'delay') return `${Math.max(0, Number(step.seconds) || 0)}s wait`;
            return type || 'step';
        }).join(' -> ');

        return `${steps.length} step${steps.length !== 1 ? 's' : ''}${preview ? ` | ${preview}` : ''}`;
    } catch {
        return 'Scenario';
    }
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
    const titleEl = document.getElementById('modal-title');
    const nameEl = document.getElementById('m-name');
    const cronEl = document.getElementById('m-cron');
    const typeEl = document.getElementById('m-type');
    const enabledEl = document.getElementById('m-enabled');
    const cmdEl = document.getElementById('m-cmd');
    const actionEl = document.getElementById('m-action');
    const prefixEl = document.getElementById('m-prefix');

    if (!titleEl || !nameEl || !cronEl || !typeEl || !enabledEl || !cmdEl || !actionEl || !prefixEl) {
        console.warn('Schedules modal elements are missing in DOM');
        showToast('danger', 'Schedule modal is not ready. Reload the page.');
        return;
    }

    if (!_modal) {
        const modalEl = document.getElementById('schedModal');
        if (!modalEl) {
            showToast('danger', 'Schedule modal is missing. Reload the page.');
            return;
        }
        _modal = new bootstrap.Modal(modalEl);
    }

    _editingId = sched ? sched.id : null;
    titleEl.innerHTML =
        `<i class="bi bi-calendar-event me-2" style="color:#a78bfa;"></i>${sched ? 'Edit Schedule' : 'New Schedule'}`;
    nameEl.value = sched ? sched.name : '';
    cronEl.value = sched ? sched.cron_expression : '0 3 * * *';
    typeEl.value = sched ? sched.task_type : 'command';
    enabledEl.checked = sched ? sched.enabled : true;

    _renderScriptSteps([]);
    if (sched) {
        try {
            const p = JSON.parse(sched.task_payload || '{}');
            cmdEl.value = p.cmd || '';
            actionEl.value = p.action || 'restart';
            prefixEl.value = p.prefix || 'backup';
            if (sched.task_type === 'script') {
                _renderScriptSteps(Array.isArray(p.steps) ? p.steps : []);
            }
        } catch { }
    } else {
        cmdEl.value = '';
        actionEl.value = 'restart';
        prefixEl.value = 'backup';
        _renderScriptSteps([{ type: 'command', cmd: '' }]);
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
    document.getElementById('payload-script').style.display  = t === 'script'  ? '' : 'none';

    if (t === 'script' && _scriptStepsEl() && !_scriptStepsEl().children.length) {
        _renderScriptSteps([{ type: 'command', cmd: '' }]);
    }
}

function _reindexScriptSteps(host = _scriptStepsEl()) {
    if (!host) return;
    Array.from(host.querySelectorAll('.con-script-step')).forEach((row, index) => {
        const stepNo = index + 1;
        row.dataset.stepNo = String(stepNo);
        const noEl = row.querySelector('.con-script-step-no');
        if (noEl) noEl.textContent = String(stepNo);
    });
}

function _normalizeScriptStep(step = {}) {
    const type = String(step.type || 'command').toLowerCase();
    if (type === 'delay') {
        return { type: 'delay', seconds: Number(step.seconds || 0) };
    }
    if (type === 'power') {
        return { type: 'power', action: String(step.action || 'restart') };
    }
    if (type === 'backup') {
        return { type: 'backup', prefix: String(step.prefix || 'backup') };
    }
    return { type: 'command', cmd: String(step.cmd || '') };
}

function _renderScriptSteps(steps) {
    const host = _scriptStepsEl();
    if (!host) return;
    const items = Array.isArray(steps) && steps.length ? steps.map(_normalizeScriptStep) : [];
    host.innerHTML = '';
    const initial = items.length ? items : [{ type: 'command', cmd: '' }];
    initial.forEach((step, index) => host.appendChild(_createScriptStepRow(step, index)));
    _reindexScriptSteps(host);
}

function _createScriptStepRow(step = {}, index = 0) {
    const stepNo = index + 1;
    const row = document.createElement('div');
    row.className = 'con-script-step';
    row.dataset.stepType = step.type || 'command';
    row.dataset.stepNo = String(stepNo);

    const head = document.createElement('div');
    head.className = 'con-script-step-head';

    const headLeft = document.createElement('div');

    const tag = document.createElement('div');
    tag.className = 'con-script-step-tag';

    const no = document.createElement('span');
    no.className = 'con-script-step-no';
    no.textContent = String(stepNo);

    const kind = document.createElement('span');
    kind.className = 'con-script-step-kind';
    kind.textContent = _SCRIPT_STEP_INFO[step.type || 'command']?.label || 'Command';

    tag.appendChild(no);
    tag.appendChild(kind);

    const hint = document.createElement('div');
    hint.className = 'con-script-step-hint';
    hint.textContent = _SCRIPT_STEP_INFO[step.type || 'command']?.hint || '';

    headLeft.appendChild(tag);
    headLeft.appendChild(hint);

    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'con-script-step-remove';
    remove.innerHTML = '<i class="bi bi-trash3"></i>';
    remove.title = 'Remove step';
    remove.addEventListener('click', () => {
        const host = _scriptStepsEl();
        if (!host) return;
        row.remove();
        if (!host.children.length) {
            _renderScriptSteps([{ type: 'command', cmd: '' }]);
            return;
        }
        _reindexScriptSteps(host);
    });

    head.appendChild(headLeft);
    head.appendChild(remove);

    const fields = document.createElement('div');
    fields.className = 'con-script-step-fields';

    const type = document.createElement('select');
    type.className = 'form-input';
    type.dataset.stepField = 'type';
    type.innerHTML = `
        <option value="command">Command</option>
        <option value="power">Power</option>
        <option value="backup">Backup</option>
        <option value="delay">Delay</option>
    `;
    type.value = step.type || 'command';

    const cmd = document.createElement('input');
    cmd.type = 'text';
    cmd.className = 'form-input con-script-step-value';
    cmd.dataset.stepField = 'command';
    cmd.placeholder = 'Shell command';
    cmd.value = step.cmd || '';

    const action = document.createElement('select');
    action.className = 'form-input con-script-step-value';
    action.dataset.stepField = 'action';
    action.innerHTML = `
        <option value="start">Start</option>
        <option value="stop">Stop</option>
        <option value="restart">Restart</option>
        <option value="kill">Kill</option>
    `;
    action.value = step.action || 'restart';

    const prefix = document.createElement('input');
    prefix.type = 'text';
    prefix.className = 'form-input con-script-step-value';
    prefix.dataset.stepField = 'prefix';
    prefix.placeholder = 'Backup prefix';
    prefix.value = step.prefix || 'backup';

    const seconds = document.createElement('input');
    seconds.type = 'number';
    seconds.min = '0';
    seconds.step = '1';
    seconds.className = 'form-input con-script-step-value';
    seconds.dataset.stepField = 'seconds';
    seconds.placeholder = 'Seconds';
    seconds.value = Number.isFinite(Number(step.seconds)) ? String(step.seconds) : '0';

    fields.appendChild(type);
    fields.appendChild(cmd);
    fields.appendChild(action);
    fields.appendChild(prefix);
    fields.appendChild(seconds);

    function updateKind(typeValue) {
        const info = _SCRIPT_STEP_INFO[typeValue] || _SCRIPT_STEP_INFO.command;
        row.dataset.stepType = typeValue;
        kind.textContent = info.label;
        hint.textContent = info.hint;
    }

    function syncFields() {
        const t = type.value;
        updateKind(t);
        cmd.hidden = t !== 'command';
        action.hidden = t !== 'power';
        prefix.hidden = t !== 'backup';
        seconds.hidden = t !== 'delay';
    }

    type.addEventListener('change', syncFields);
    syncFields();

    row.appendChild(head);
    row.appendChild(fields);
    return row;
}

function addScriptStepRow() {
    const host = _scriptStepsEl();
    if (!host) return;
    host.appendChild(_createScriptStepRow({ type: 'command', cmd: '' }, host.children.length));
    _reindexScriptSteps(host);
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
    if (type === 'script') {
        const steps = Array.from((_scriptStepsEl()?.querySelectorAll('.con-script-step')) || []).map(row => {
            const stepType = row.querySelector('[data-step-field="type"]')?.value || 'command';
            if (stepType === 'command') {
                return { type: 'command', cmd: row.querySelector('[data-step-field="command"]')?.value.trim() || '' };
            }
            if (stepType === 'power') {
                return { type: 'power', action: row.querySelector('[data-step-field="action"]')?.value || 'restart' };
            }
            if (stepType === 'backup') {
                return { type: 'backup', prefix: row.querySelector('[data-step-field="prefix"]')?.value.trim() || 'backup' };
            }
            return {
                type: 'delay',
                seconds: Math.max(0, parseInt(row.querySelector('[data-step-field="seconds"]')?.value || '0', 10) || 0),
            };
        }).filter(step => {
            if (step.type === 'command') return !!step.cmd;
            if (step.type === 'delay') return Number(step.seconds) > 0;
            return true;
        });
        if (!steps.length) { showToast('danger', 'Script needs at least one step'); return; }
        payload = { steps };
    }

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
