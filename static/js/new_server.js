// ── Row templates ────────────────────────────────────────────────────────────

function getPortRowHtml(host='', container='', proto='tcp') {
    const opt = v => `<option value="${v}"${proto===v?' selected':''}>`;
    return `
    <div class="entry-row" style="display:grid;grid-template-columns:1fr 28px 1fr 84px 26px;gap:.45rem;align-items:center;">
        <input type="number" min="1" max="65535" class="form-control host-input" placeholder="20000" value="${host}" oninput="updateYaml()">
        <span class="row-sep" style="text-align:center;">:</span>
        <input type="number" min="1" max="65535" class="form-control container-input" placeholder="20000" value="${container}" oninput="updateYaml()">
        <select class="form-select proto-sel proto-input" onchange="updateYaml()">
            ${opt('tcp')}TCP</option>
            ${opt('udp')}UDP</option>
            ${opt('tcp+udp')}Both</option>
        </select>
        <button type="button" class="row-del" onclick="this.closest('.entry-row').remove();updateYaml()" title="Remove">
            <i class="bi bi-trash3"></i>
        </button>
    </div>`;
}

function getEnvRowHtml(key='', val='') {
    return `
    <div class="entry-row" style="display:grid;grid-template-columns:1fr 20px 1fr 26px;gap:.45rem;align-items:center;">
        <input type="text" class="form-control key-input" placeholder="VARIABLE" value="${key}" oninput="updateYaml()">
        <span class="row-sep" style="text-align:center;">=</span>
        <input type="text" class="form-control val-input" placeholder="value" value="${val}" oninput="updateYaml()">
        <button type="button" class="row-del" onclick="this.closest('.entry-row').remove();updateYaml()" title="Remove">
            <i class="bi bi-trash3"></i>
        </button>
    </div>`;
}

function getVolRowHtml(host='', container='') {
    return `
    <div class="entry-row" style="display:grid;grid-template-columns:1fr 20px 1fr 26px;gap:.45rem;align-items:center;">
        <input type="text" class="form-control host-input" placeholder="/host/path" value="${host}" oninput="updateYaml()">
        <span class="row-sep" style="text-align:center;">:</span>
        <input type="text" class="form-control container-input" placeholder="/data" value="${container}" oninput="updateYaml()">
        <button type="button" class="row-del" onclick="this.closest('.entry-row').remove();updateYaml()" title="Remove">
            <i class="bi bi-trash3"></i>
        </button>
    </div>`;
}

// ── Add helpers ──────────────────────────────────────────────────────────────

function addPortRow(h, c, p) {
    document.getElementById('ports-container').insertAdjacentHTML('beforeend', getPortRowHtml(h, c, p));
    if (h === undefined) updateYaml();
}
function addEnvRow(k, v) {
    document.getElementById('env-container').insertAdjacentHTML('beforeend', getEnvRowHtml(k, v));
    if (k === undefined) updateYaml();
}
function addVolRow(h, c) {
    document.getElementById('vol-container').insertAdjacentHTML('beforeend', getVolRowHtml(h, c));
    if (h === undefined) updateYaml();
}

// ── YAML generation ──────────────────────────────────────────────────────────

function updateYaml() {
    const config = {
        image:       document.getElementById('image').value || undefined,
        restart:     document.getElementById('gui_restart').value,
        ports:       [],
        environment: [],
        volumes:     []
    };

    const cpus = document.getElementById('gui_cpus').value;
    if (cpus) config.cpus = parseFloat(cpus);

    const memVal = document.getElementById('gui_mem_val').value;
    if (memVal) config.mem_limit = memVal + document.getElementById('gui_mem_unit').value;

    const diskVal = document.getElementById('gui_disk_val').value;
    if (diskVal) config.disk_limit = diskVal + document.getElementById('gui_disk_unit').value;

    document.querySelectorAll('#ports-container .entry-row').forEach(row => {
        const h = row.querySelector('.host-input').value;
        const c = row.querySelector('.container-input').value;
        const p = row.querySelector('.proto-input').value;
        if (h && c) config.ports.push(`${h}:${c}${p ? '/'+p : ''}`);
    });

    document.querySelectorAll('#env-container .entry-row').forEach(row => {
        const k = row.querySelector('.key-input').value;
        const v = row.querySelector('.val-input').value;
        if (k) config.environment.push(`${k}=${v}`);
    });

    document.querySelectorAll('#vol-container .entry-row').forEach(row => {
        const h = row.querySelector('.host-input').value;
        const c = row.querySelector('.container-input').value;
        if (h && c) config.volumes.push(`${h}:${c}`);
    });

    if (!config.ports.length)       delete config.ports;
    if (!config.environment.length) delete config.environment;
    if (!config.volumes.length)     delete config.volumes;
    if (!config.image)              delete config.image;

    const yamlStr = jsyaml.dump(config, { indent: 2, lineWidth: -1 });
    document.getElementById('config').value = yamlStr;
    if (window.yamlEditor) {
        const pos = window.yamlEditor.getPosition();
        window.yamlEditor.setValue(yamlStr);
        if (pos) window.yamlEditor.setPosition(pos);
    }
    saveFormState();
    // Keep port select in sync
    _refreshSrvPortSelect();
}

function copyYaml(btn) {
    const val = document.getElementById('config').value;
    if (!val) return;
    navigator.clipboard.writeText(val).then(() => {
        btn.innerHTML = '<i class="bi bi-check2"></i> Copied!';
        btn.style.color = 'var(--success)';
        setTimeout(() => { btn.innerHTML = '<i class="bi bi-clipboard"></i> Copy'; btn.style.color = ''; }, 2000);
    });
}

// ── Image datalist ───────────────────────────────────────────────────────────

fetch('/api/image/local').then(r => r.json()).then(d => {
    const dl = document.getElementById('local-images-list');
    (d.tags || []).forEach(t => {
        const opt = document.createElement('option');
        opt.value = t;
        dl.appendChild(opt);
    });
}).catch(() => {});

// ── Fetch ENV ────────────────────────────────────────────────────────────────

async function fetchImageEnv() {
    const image = document.getElementById('image').value.trim();
    if (!image) { alert('Enter a Docker image name first.'); return; }
    const btn    = document.getElementById('fetch-env-btn');
    const status = document.getElementById('fetch-env-status');
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" style="width:.8rem;height:.8rem;"></span> Loading…';
    status.style.color = '';
    status.textContent = '';
    try {
        const enc = encodeURIComponent(image);
        const overridesRes = await fetch(`/api/image/env-overrides?image=${enc}`).then(r => r.json()).catch(() => ({ ok: false, env: '' }));
        const dbEnv = (overridesRes.ok && overridesRes.env) ? overridesRes.env.trim() : '';

        const map = new Map();
        if (dbEnv) {
            for (const line of dbEnv.split('\n')) {
                const t = line.trim(); if (!t) continue;
                const eq = t.indexOf('=');
                map.set(eq >= 0 ? t.slice(0, eq) : t, eq >= 0 ? t.slice(eq + 1) : '');
            }
        } else {
            btn.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" style="width:.8rem;height:.8rem;"></span> Pulling image…';
            const nativeRes = await fetch(`/api/image/env?image=${enc}`).then(r => r.json());
            if (!nativeRes.ok) throw new Error(nativeRes.error || 'Unknown error');
            for (const pair of (nativeRes.env || [])) {
                const eq = pair.indexOf('=');
                map.set(eq >= 0 ? pair.slice(0, eq) : pair, eq >= 0 ? pair.slice(eq + 1) : '');
            }
        }

        const existing = new Set(
            Array.from(document.querySelectorAll('#env-container .key-input')).map(el => el.value)
        );
        let added = 0;
        for (const [k, v] of map) {
            if (!existing.has(k)) { addEnvRow(k, v); existing.add(k); added++; }
        }
        status.style.color = 'var(--success)';
        status.textContent = added > 0
            ? `✓ Added ${added} var${added !== 1 ? 's' : ''}${dbEnv ? ' (from DB)' : ''}`
            : '✓ No new vars';
    } catch (e) {
        status.style.color = 'var(--danger)';
        status.textContent = `✗ ${e.message}`;
    } finally {
        btn.disabled = false;
        btn.innerHTML = '<i class="bi bi-cloud-download"></i> Fetch ENV';
        setTimeout(() => { status.textContent = ''; }, 4000);
    }
}

// ── Monaco init ──────────────────────────────────────────────────────────────

require.config({ paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs' } });
require(['vs/editor/editor.main'], function () {
    window.yamlEditor = monaco.editor.create(document.getElementById('yaml-editor-container'), {
        value: '',
        language: 'yaml',
        theme: 'vs-dark',
        automaticLayout: true,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
        fontSize: 12,
        lineHeight: 19,
        lineNumbers: 'on',
        renderLineHighlight: 'gutter',
        roundedSelection: true,
        scrollbar: { verticalScrollbarSize: 4, horizontalScrollbarSize: 4 },
        padding: { top: 10, bottom: 10 },
        wordWrap: 'off',
        overviewRulerLanes: 0,
        hideCursorInOverviewRuler: true,
        overviewRulerBorder: false,
        glyphMargin: false,
        folding: true,
        renderWhitespace: 'none',
        bracketPairColorization: { enabled: true },
    });
    window.yamlEditor.onDidChangeModelContent(() => {
        document.getElementById('config').value = window.yamlEditor.getValue();
    });
    updateYaml();
});

// ── Helpers ──────────────────────────────────────────────────────────────────
function esc(s) {
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;')
        .replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
}

const _adjectives = ['fast','cool','epic','dark','blue','red','gold','iron','neon','soft','wild','bold','calm','lazy','tiny'];
const _nouns      = ['server','node','box','host','core','unit','base','hub','rack','cloud','forge','block','spark','tower','realm'];
function randomServerName() {
    const a = _adjectives[Math.floor(Math.random() * _adjectives.length)];
    const n = _nouns[Math.floor(Math.random() * _nouns.length)];
    const d = Math.floor(1000 + Math.random() * 9000);
    return `${a}-${n}-${d}`;
}
function autoFillName() {
    const el = document.getElementById('name');
    if (!el.value.trim()) el.value = randomServerName();
}

// ── Form persistence (sessionStorage) ───────────────────────────────────
const FORM_KEY = 'yunexal_new_server_draft';

function saveFormState() {
    const ports = [], envs = [], vols = [];
    document.querySelectorAll('#ports-container .entry-row').forEach(r => {
        ports.push({ h: r.querySelector('.host-input').value, c: r.querySelector('.container-input').value, p: r.querySelector('.proto-input').value });
    });
    document.querySelectorAll('#env-container .entry-row').forEach(r => {
        envs.push({ k: r.querySelector('.key-input').value, v: r.querySelector('.val-input').value });
    });
    document.querySelectorAll('#vol-container .entry-row').forEach(r => {
        vols.push({ h: r.querySelector('.host-input').value, c: r.querySelector('.container-input').value });
    });
    const collapsed = {};
    document.querySelectorAll('.sec-card').forEach((card, i) => {
        const bd = card.querySelector('.sec-bd');
        if (bd) collapsed[i] = bd.style.display === 'none';
    });
    sessionStorage.setItem(FORM_KEY, JSON.stringify({
        name:          document.getElementById('name')?.value || '',
        owner_id:      document.getElementById('owner_id')?.value || '0',
        image:         document.getElementById('image')?.value || '',
        gui_cpus:      document.getElementById('gui_cpus')?.value || '',
        gui_mem_val:   document.getElementById('gui_mem_val')?.value || '',
        gui_mem_unit:  document.getElementById('gui_mem_unit')?.value || 'mb',
        gui_disk_val:  document.getElementById('gui_disk_val')?.value || '',
        gui_disk_unit: document.getElementById('gui_disk_unit')?.value || 'gb',
        bandwidth_mbit:document.getElementById('bandwidth_mbit')?.value || '',
        gui_restart:   document.getElementById('gui_restart')?.value || 'unless-stopped',
        ports, envs, vols, collapsed,
        srv_enabled:     document.getElementById('srv-enable-chk')?.checked || false,
        srv_service:     document.getElementById('srv-service')?.value || '',
        srv_a_subdomain: document.getElementById('srv-a-subdomain')?.value || '',
        srv_port:        document.getElementById('srv-port')?.value || '',
        dns_open:        document.getElementById('dns-srv-body')?.style.display !== 'none',
    }));
}

function restoreFormState() {
    const raw = sessionStorage.getItem(FORM_KEY);
    if (!raw) return;
    let s; try { s = JSON.parse(raw); } catch { return; }
    const sv = (id, val) => { const el = document.getElementById(id); if (el && val !== undefined && val !== '') el.value = val; };
    sv('name', s.name);
    sv('owner_id', s.owner_id);
    sv('image', s.image);
    sv('gui_cpus', s.gui_cpus);
    sv('gui_mem_val', s.gui_mem_val);
    sv('gui_mem_unit', s.gui_mem_unit);
    sv('gui_disk_val', s.gui_disk_val);
    sv('gui_disk_unit', s.gui_disk_unit);
    sv('bandwidth_mbit', s.bandwidth_mbit);
    sv('gui_restart', s.gui_restart);
    (s.ports || []).forEach(r => addPortRow(r.h, r.c, r.p));
    (s.envs  || []).forEach(r => addEnvRow(r.k, r.v));
    (s.vols  || []).forEach(r => addVolRow(r.h, r.c));
    if (s.collapsed) {
        document.querySelectorAll('.sec-card').forEach((card, i) => {
            if (!(i in s.collapsed)) return;
            const bd   = card.querySelector('.sec-bd');
            const chev = card.querySelector('.sec-chev') || card.querySelector('#dns-srv-chevron');
            if (!bd) return;
            bd.style.display = s.collapsed[i] ? 'none' : '';
            if (chev) chev.className = 'bi ' + (s.collapsed[i] ? 'bi-chevron-down' : 'bi-chevron-up') +
                (chev.id === 'dns-srv-chevron' ? '' : ' sec-chev');
        });
    }
    if (s.dns_open) {
        const body = document.getElementById('dns-srv-body');
        if (body) { body.style.display = ''; loadSrvProviders(); }
        const chev = document.getElementById('dns-srv-chevron');
        if (chev) chev.className = 'bi bi-chevron-up';
    }
    if (s.srv_enabled) {
        const chk = document.getElementById('srv-enable-chk');
        if (chk) { chk.checked = true; onSrvToggle(); }
        sv('srv-service', s.srv_service);
        sv('srv-a-subdomain', s.srv_a_subdomain);
        // srv-port is a <select> populated by _refreshSrvPortSelect inside onSrvToggle,
        // so restore value after a tick to ensure options exist
        if (s.srv_port) setTimeout(() => {
            const sel = document.getElementById('srv-port');
            if (sel && sel.querySelector(`option[value="${s.srv_port}"]`)) sel.value = s.srv_port;
            updateSrvPreview();
        }, 50);
        else updateSrvPreview();
    }
}

function clearFormState() {
    sessionStorage.removeItem(FORM_KEY);
}

// ── Sec-card generic collapse ───────────────────────────────────────────────
function toggleSecCard(hd) {
    const body = hd.closest('.sec-card').querySelector('.sec-bd');
    if (!body) return;
    const open = body.style.display !== 'none';
    body.style.display = open ? 'none' : '';
    const chev = hd.querySelector('.sec-chev');
    if (chev) chev.className = 'bi ' + (open ? 'bi-chevron-down' : 'bi-chevron-up') + ' sec-chev';
    if (chev) chev.style.cssText = 'margin-left:auto;color:var(--muted);transition:transform .2s;';
    saveFormState();
}

// ── Confirm Create Modal ────────────────────────────────────────────────────
function _cfmSection(icon, title, content) {
    return `<div style="background:var(--surface2);border:1px solid var(--bdr);border-radius:12px;overflow:hidden;">
        <div style="display:flex;align-items:center;gap:.5rem;padding:.6rem 1rem;border-bottom:1px solid var(--bdr);background:var(--surface3);">
            <i class="bi ${icon}" style="color:var(--accent-l);font-size:.8rem;"></i>
            <span style="font-size:.69rem;font-weight:600;text-transform:uppercase;letter-spacing:.07em;color:var(--muted);">${title}</span>
        </div>
        <div style="padding:.75rem 1rem;">${content}</div>
    </div>`;
}
function _cfmKV(label, value, accent) {
    return `<div style="display:flex;align-items:baseline;justify-content:space-between;gap:.5rem;padding:.22rem 0;border-bottom:1px solid rgba(255,255,255,.035);min-width:0;">
        <span style="font-size:.76rem;color:var(--muted);white-space:nowrap;flex-shrink:0;">${label}</span>
        <span style="font-size:.8rem;font-weight:500;color:${accent||'var(--txt)'};text-align:right;word-break:break-all;overflow-wrap:anywhere;min-width:0;">${value}</span>
    </div>`;
}
function _cfmBadge(text, color) {
    return `<span style="display:inline-block;background:${color||'rgba(124,58,237,.15)'};border:1px solid ${color ? color.replace('.15',',.35') : 'rgba(124,58,237,.25)'};color:${color ? '#fff' : 'var(--accent-l)'};font-size:.72rem;font-family:monospace;padding:.15rem .55rem;border-radius:5px;margin:.15rem .15rem 0 0;">${text}</span>`;
}
function _cfmEmpty(msg) {
    return `<span style="font-size:.75rem;color:var(--muted);font-style:italic;">${msg}</span>`;
}

function showCreateConfirm() {
    // Auto-fill name if empty
    const nameEl = document.getElementById('name');
    if (!nameEl.value.trim()) nameEl.value = randomServerName();

    // Require docker image
    const imageEl = document.getElementById('image');
    if (!imageEl.value.trim()) {
        imageEl.focus();
        imageEl.style.borderColor = 'rgba(239,68,68,.6)';
        imageEl.style.boxShadow   = '0 0 0 3px rgba(239,68,68,.15)';
        const basicBd = imageEl.closest('.sec-bd');
        if (basicBd && basicBd.style.display === 'none') {
            basicBd.style.display = '';
            const chev = basicBd.closest('.sec-card')?.querySelector('.sec-chev');
            if (chev) chev.className = 'bi bi-chevron-up sec-chev';
        }
        return;
    }
    imageEl.style.borderColor = '';
    imageEl.style.boxShadow   = '';

    updateYaml();
    prepareSrvHiddenFields();

    const name     = document.getElementById('name').value.trim() || '—';
    const ownerSel = document.getElementById('owner_id');
    const ownerTxt = ownerSel.options[ownerSel.selectedIndex]?.text || '—';
    const image    = document.getElementById('image').value.trim() || '—';
    const restart  = document.getElementById('gui_restart').value;
    const cpus     = document.getElementById('gui_cpus').value;
    const memVal   = document.getElementById('gui_mem_val').value;
    const memUnit  = document.getElementById('gui_mem_unit').value.toUpperCase();
    const diskVal  = document.getElementById('gui_disk_val').value;
    const diskUnit = document.getElementById('gui_disk_unit').value.toUpperCase();
    const bw       = document.getElementById('bandwidth_mbit').value;

    const restartColor = { 'always':'rgba(34,197,94,.15)', 'unless-stopped':'rgba(234,179,8,.15)', 'on-failure':'rgba(239,68,68,.15)', 'no':'rgba(107,114,128,.15)' }[restart] || 'rgba(124,58,237,.15)';
    const restartBorder= { 'always':'rgba(34,197,94,.35)', 'unless-stopped':'rgba(234,179,8,.35)', 'on-failure':'rgba(239,68,68,.35)', 'no':'rgba(107,114,128,.35)' }[restart] || 'rgba(124,58,237,.35)';
    const restartTxt   = { 'always':'#86efac',            'unless-stopped':'#fde047',             'on-failure':'#fca5a5',            'no':'#9ca3af'            }[restart] || 'var(--accent-l)';
    const restartBadge = `<span style="display:inline-block;background:${restartColor};border:1px solid ${restartBorder};color:${restartTxt};font-size:.72rem;padding:.15rem .55rem;border-radius:5px;">${esc(restart)}</span>`;

    const sections = [];

    // ── Basic Info ──
    sections.push(_cfmSection('bi-tag-fill', 'Basic Info',
        _cfmKV('Server Name', `<strong style="color:var(--txt);font-family:monospace;letter-spacing:.02em;">${esc(name)}</strong>`) +
        _cfmKV('Owner', esc(ownerTxt)) +
        _cfmKV('Docker Image', `<code style="color:#a78bfa;font-size:.78rem;">${esc(image)}</code>`)
    ));

    // ── Resources ──
    sections.push(_cfmSection('bi-cpu-fill', 'Resources & Limits',
        _cfmKV('CPU',       cpus    ? `<span style="color:var(--success);">${esc(cpus)} core${parseFloat(cpus)!==1?'s':''}</span>` : _cfmEmpty('Unlimited')) +
        _cfmKV('RAM',       memVal  ? `<span style="color:var(--success);">${esc(memVal)}\u202f${memUnit}</span>` : _cfmEmpty('Unlimited')) +
        _cfmKV('Disk',      diskVal ? `<span style="color:var(--success);">${esc(diskVal)}\u202f${diskUnit}</span>` : _cfmEmpty('Unlimited')) +
        _cfmKV('Bandwidth', bw      ? `<span style="color:var(--success);">${esc(bw)} Mbit/s</span>` : _cfmEmpty('Unlimited')) +
        _cfmKV('Restart',   restartBadge)
    ));

    // ── Ports ──
    const ports = [];
    document.querySelectorAll('#ports-container .entry-row').forEach(r => {
        const h = r.querySelector('.host-input').value;
        const c = r.querySelector('.container-input').value;
        const p = r.querySelector('.proto-input').value;
        if (h && c) ports.push({ h, c, p });
    });
    {
        const inner = ports.length
            ? `<div style="display:grid;grid-template-columns:auto auto auto;gap:.3rem .6rem;align-items:center;font-size:.78rem;font-family:monospace;">
                <span style="font-size:.67rem;font-weight:600;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);">Host</span>
                <span style="font-size:.67rem;font-weight:600;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);">Container</span>
                <span style="font-size:.67rem;font-weight:600;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);">Proto</span>
                ${ports.map(({h,c,p}) =>
                    `<span style="color:#a78bfa;">${esc(h)}</span><span style="color:var(--txt);">→ ${esc(c)}</span><span style="background:rgba(124,58,237,.15);border:1px solid rgba(124,58,237,.25);color:var(--accent-l);padding:.1rem .4rem;border-radius:4px;font-size:.7rem;">${esc(p)}</span>`
                ).join('')}
            </div>`
            : _cfmEmpty('No port bindings configured');
        sections.push(_cfmSection('bi-diagram-2-fill', `Port Bindings${ports.length ? ' ('+ports.length+')' : ''}`, inner));
    }

    // ── Environment ──
    const envs = [];
    document.querySelectorAll('#env-container .entry-row').forEach(r => {
        const k = r.querySelector('.key-input').value;
        const v = r.querySelector('.val-input').value;
        if (k) envs.push({ k, v });
    });
    {
        const inner = envs.length
            ? `<div style="display:flex;flex-direction:column;gap:.25rem;">${envs.map(({k,v}) =>
                `<div style="display:flex;align-items:baseline;gap:.4rem;padding:.25rem .4rem;background:var(--surface3);border-radius:6px;font-family:monospace;font-size:.77rem;overflow:hidden;">
                    <span style="color:#a78bfa;white-space:nowrap;flex-shrink:0;">${esc(k)}</span>
                    <span style="color:var(--muted);flex-shrink:0;">=</span>
                    <span style="color:var(--txt);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${esc(v)||'<em style="opacity:.5;">empty</em>'}</span>
                </div>`
            ).join('')}</div>`
            : _cfmEmpty('No environment variables');
        sections.push(_cfmSection('bi-code-square', `Environment${envs.length ? ' ('+envs.length+')' : ''}`, inner));
    }

    // ── Volumes ──
    const vols = [];
    document.querySelectorAll('#vol-container .entry-row').forEach(r => {
        const h = r.querySelector('.host-input').value;
        const c = r.querySelector('.container-input').value;
        if (h && c) vols.push({ h, c });
    });
    {
        const inner = vols.length
            ? `<div style="display:flex;flex-direction:column;gap:.25rem;">${vols.map(({h,c}) =>
                `<div style="display:flex;align-items:center;gap:.4rem;padding:.25rem .4rem;background:var(--surface3);border-radius:6px;font-family:monospace;font-size:.77rem;overflow:hidden;">
                    <span style="color:var(--txt);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1;">${esc(h)}</span>
                    <i class="bi bi-arrow-right" style="color:var(--muted);flex-shrink:0;font-size:.7rem;"></i>
                    <span style="color:#a78bfa;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1;">${esc(c)}</span>
                </div>`
            ).join('')}</div>`
            : _cfmEmpty('No volume mounts');
        sections.push(_cfmSection('bi-hdd-fill', `Volume Mounts${vols.length ? ' ('+vols.length+')' : ''}`, inner));
    }

    // ── DNS / SRV ──
    const srvEnabled = document.getElementById('h-dns-srv-enabled').value === '1';
    if (srvEnabled) {
        const srvName   = document.getElementById('h-dns-srv-name').value || '_yunexal';
        const srvTarget = document.getElementById('h-dns-srv-target').value;
        const srvPort   = document.getElementById('h-dns-srv-port').value;
        const aSub      = document.getElementById('h-dns-a-subdomain').value;
        const aIp       = document.getElementById('h-dns-a-ip').value;
        const proto     = document.getElementById('h-dns-srv-both-protos').value || 'both';
        const zone      = document.getElementById('h-dns-zone-name').value;
        const protoLabel = proto === 'tcp' ? 'TCP only' : proto === 'udp' ? 'UDP only' : 'TCP + UDP';
        const protoColor = proto === 'both' ? '#34d399' : '#fbbf24';
        const records = [];
        if (proto === 'both' || proto === 'tcp') records.push(`<code style="color:#a78bfa;font-size:.77rem;">${esc(srvName)}._tcp.${esc(srvTarget)}</code>`);
        if (proto === 'both' || proto === 'udp') records.push(`<code style="color:#a78bfa;font-size:.77rem;">${esc(srvName)}._udp.${esc(srvTarget)}</code>`);
        sections.push(_cfmSection('bi-hdd-network-fill', 'DNS / SRV Record',
            _cfmKV('Protocol', `<span style="color:${protoColor};font-size:.77rem;font-weight:600;">${protoLabel}</span>`) +
            records.map((r, i) => _cfmKV(records.length > 1 ? `SRV record ${i+1}` : 'SRV record', r)).join('') +
            _cfmKV('Target', `<code style="color:var(--txt);font-size:.77rem;">${esc(srvTarget)}:${esc(srvPort)}</code>`) +
            (zone ? _cfmKV('Zone', `<code style="color:var(--muted);font-size:.77rem;">${esc(zone)}</code>`) : '') +
            (aSub ? _cfmKV('A-record', `<code style="color:#a78bfa;font-size:.77rem;">${esc(aSub)}.${esc(zone)}</code>${aIp ? ` <span style="color:var(--muted);font-size:.72rem;">→ ${esc(aIp)}</span>` : ''}`) : '')
        ));
    }

    document.getElementById('confirm-summary').innerHTML = sections.join('');
    document.getElementById('confirmCreateModal').style.display = 'block';}

function hideCreateConfirm() {
    document.getElementById('confirmCreateModal').style.display = 'none';
}

function submitCreateForm() {
    clearFormState();
    document.getElementById('createServerForm').submit();
}

// ── DNS / SRV ──────────────────────────────────────────────────────────────────
// ── DNS / SRV ─────────────────────────────────────────────────────────────────
function toggleDnsSrv() {
    const body = document.getElementById('dns-srv-body');
    const chev = document.getElementById('dns-srv-chevron');
    const open = body.style.display !== 'none';
    body.style.display = open ? 'none' : '';
    chev.className = open ? 'bi bi-chevron-down' : 'bi bi-chevron-up';
    if (!open) loadSrvProviders();
    saveFormState();
}

function onSrvToggle() {
    const on = document.getElementById('srv-enable-chk').checked;
    document.getElementById('srv-fields').style.display = on ? '' : 'none';
    saveFormState();
    if (on) {
        if (!document.getElementById('srv-service').value) document.getElementById('srv-service').value = 'yunexal';
        if (!document.getElementById('srv-a-subdomain').value) rollSrvSubdomain();
        _refreshSrvPortSelect();
    }
}

function rollSrvSubdomain() {
    const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
    let s = '';
    for (let i = 0; i < 8; i++) s += chars[Math.floor(Math.random() * chars.length)];
    document.getElementById('srv-a-subdomain').value = s;
    updateSrvPreview();
}

function _refreshSrvPortSelect() {
    const sel = document.getElementById('srv-port');
    if (!sel) return;
    const prev = sel.value;
    sel.innerHTML = '<option value="">\u2014 select port \u2014</option>';
    const seen = new Set();
    document.querySelectorAll('#ports-container .entry-row').forEach(row => {
        const v = row.querySelector('.host-input')?.value.trim();
        if (!v || seen.has(v)) return;
        seen.add(v);
        const proto = row.querySelector('.proto-input')?.value || 'tcp';
        const o = document.createElement('option');
        o.value = v;
        o.textContent = v;
        o.dataset.proto = proto;
        sel.appendChild(o);
    });
    if (sel.querySelector(`option[value="${prev}"]`)) sel.value = prev;
    updateSrvPreview();
}

function loadSrvProviders() {
    fetch('/api/admin/dns/providers', { credentials: 'same-origin' })
        .then(r => r.json()).then(d => {
            const sel = document.getElementById('srv-provider');
            sel.innerHTML = '<option value="">\u2014 Select provider \u2014</option>';
            (d.providers || []).forEach(p => {
                const o = document.createElement('option');
                o.value = p.id; o.textContent = p.name;
                sel.appendChild(o);
            });
        }).catch(() => {});
}

function onSrvProviderChange() {
    const pid = document.getElementById('srv-provider').value;
    const sel = document.getElementById('srv-zone');
    if (!pid) { sel.innerHTML = '<option value="">Select provider first</option>'; updateSrvPreview(); return; }
    sel.innerHTML = '<option value="">Loading\u2026</option>';
    fetch(`/api/admin/dns/providers/${pid}/zones`, { credentials: 'same-origin' })
        .then(r => r.json()).then(d => {
            sel.innerHTML = '<option value="">\u2014 Select domain \u2014</option>';
            (d.zones || []).forEach(z => {
                const o = document.createElement('option');
                o.value = z.id; o.textContent = z.name; o.dataset.name = z.name;
                sel.appendChild(o);
            });
            sel.onchange = () => { _autoFetchIp(); updateSrvPreview(); };
        }).catch(() => { sel.innerHTML = '<option value="">Failed to load</option>'; });
}

function _getSrvZoneName() {
    const zSel = document.getElementById('srv-zone');
    const idx = zSel.selectedIndex;
    return idx > 0 ? (zSel.options[idx]?.dataset?.name || zSel.options[idx]?.text || '') : '';
}

function _autoFetchIp() {
    fetch('/api/admin/dns/public-ip', { credentials: 'same-origin' })
        .then(r => r.json())
        .then(d => { if (d.ip) document.getElementById('h-dns-a-ip').value = d.ip; })
        .catch(() => {});
}

function _srvContainerName() {
    return (document.getElementById('name')?.value || 'server')
        .trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'server';
}

function _getSrvProto() {
    const sel = document.getElementById('srv-port');
    const p = sel?.options[sel.selectedIndex]?.dataset?.proto || 'tcp';
    return p === 'tcp+udp' ? 'both' : p;
}

function updateSrvPreview() {
    const svc   = (document.getElementById('srv-service')?.value.trim() || 'yunexal').replace(/[^a-z0-9-]/gi, '-').toLowerCase();
    const sub   = document.getElementById('srv-a-subdomain').value.trim();
    const zone  = _getSrvZoneName();
    const port  = document.getElementById('srv-port').value;
    const proto = _getSrvProto();
    const box   = document.getElementById('srv-preview-box');
    if (!box) return;
    if (!sub || !zone || !port) {
        box.innerHTML = '<em style="color:var(--muted);font-size:.74rem;">Fill in service, subdomain, domain and port to see preview</em>';
        return;
    }
    const target = `${sub}.${zone}`;
    const arrow  = `<span style="color:var(--muted);font-size:.72rem;"> \u2192 ${esc(target)}:${esc(port)}</span>`;
    const rows = [];
    if (proto === 'both' || proto === 'tcp') rows.push(`<code style="color:#a78bfa;font-size:.76rem;">_${esc(svc)}._tcp.${esc(target)}</code>${arrow}`);
    if (proto === 'both' || proto === 'udp') rows.push(`<code style="color:#a78bfa;font-size:.76rem;">_${esc(svc)}._udp.${esc(target)}</code>${arrow}`);
    box.innerHTML = rows.map(r => `<div style="margin-bottom:.15rem;">${r}</div>`).join('');
}

function prepareSrvHiddenFields() {
    const chk = document.getElementById('srv-enable-chk');
    if (!chk || !chk.checked) { document.getElementById('h-dns-srv-enabled').value = '0'; return; }
    const pid   = document.getElementById('srv-provider').value;
    const zSel  = document.getElementById('srv-zone');
    const zid   = zSel.value;
    const zname = zSel.options[zSel.selectedIndex]?.dataset?.name || zSel.options[zSel.selectedIndex]?.text || zid;
    const port  = document.getElementById('srv-port').value;
    const aSub  = document.getElementById('srv-a-subdomain').value.trim();
    if (!pid || !zid || !port || !aSub) { document.getElementById('h-dns-srv-enabled').value = '0'; return; }
    const svc    = (document.getElementById('srv-service')?.value.trim() || 'yunexal').replace(/[^a-z0-9-]/gi, '-').toLowerCase();
    const target = `${aSub}.${zname}`;
    const proto = _getSrvProto();
    document.getElementById('h-dns-srv-enabled').value     = '1';
    document.getElementById('h-dns-srv-both-protos').value = proto;
    document.getElementById('h-dns-provider-id').value     = pid;
    document.getElementById('h-dns-zone-id').value         = zid;
    document.getElementById('h-dns-zone-name').value       = zname;
    document.getElementById('h-dns-srv-name').value        = `_${svc}`;
    document.getElementById('h-dns-srv-port').value        = port;
    document.getElementById('h-dns-srv-target').value      = target;
    document.getElementById('h-dns-srv-priority').value    = '0';
    document.getElementById('h-dns-srv-weight').value      = '0';
    document.getElementById('h-dns-a-subdomain').value     = aSub;
    // h-dns-a-ip already set by _autoFetchIp()
}
