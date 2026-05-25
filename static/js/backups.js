// Backups page
const BID = window.YU_SERVER_ID;
let _bModal = null;
let _restoreModal = null;
let _backingUp = false;

window._yuPageCleanup = function () {};

// ── Modals (injected into <body> on demand) ────────────────────────────────────

const _MODAL_HTML = `
<div class="modal fade" id="createBackupModal" tabindex="-1">
    <div class="modal-dialog modal-dialog-centered" style="max-width:420px;">
        <div class="modal-content" style="background:#111119;border:1px solid rgba(16,185,129,.2);border-radius:14px;">
            <div class="modal-header" style="border-bottom:1px solid rgba(255,255,255,.07);">
                <h6 class="modal-title" style="font-weight:600;color:var(--txt);"><i class="bi bi-archive me-2" style="color:#10b981;"></i>Create Backup</h6>
                <button type="button" class="btn-close btn-close-white" data-bs-dismiss="modal"></button>
            </div>
            <div class="modal-body" style="display:flex;flex-direction:column;gap:1rem;">
                <div>
                    <label class="form-label" style="font-size:.78rem;font-weight:600;color:var(--muted);text-transform:uppercase;letter-spacing:.06em;">Filename Prefix</label>
                    <input type="text" id="b-prefix" class="form-input" placeholder="backup" value="backup">
                    <p style="font-size:.75rem;color:var(--muted);margin:.3rem 0 0;">Saved as <code style="font-size:.73rem;">backups/&lt;prefix&gt;-&lt;timestamp&gt;.tar.gz</code></p>
                </div>
                <div style="font-size:.8rem;color:var(--muted);background:rgba(255,255,255,.04);border-radius:7px;padding:.6rem .8rem;">
                    <i class="bi bi-info-circle" style="margin-right:.3rem;color:#a78bfa;"></i>
                    The backup runs synchronously — the button stays disabled until it completes.
                </div>
            </div>
            <div class="modal-footer d-flex gap-2 justify-content-end" style="border-top:1px solid rgba(255,255,255,.07);">
                <button type="button" class="btn-yu btn-yu-ghost" style="width:auto;" data-bs-dismiss="modal">Cancel</button>
                <button type="button" id="b-create-btn" class="btn-yu btn-yu-primary" style="width:auto;background:#10b981;border-color:#10b981;" onclick="createBackup()">
                    <i class="bi bi-archive"></i> Create Backup
                </button>
            </div>
        </div>
    </div>
</div>`;

const _RESTORE_MODAL_HTML = `
<div class="modal fade" id="restoreBackupModal" tabindex="-1">
    <div class="modal-dialog modal-dialog-centered" style="max-width:440px;">
        <div class="modal-content" style="background:#111119;border:1px solid rgba(248,113,113,.25);border-radius:14px;">
            <div class="modal-header" style="border-bottom:1px solid rgba(255,255,255,.07);">
                <h6 class="modal-title" style="font-weight:600;color:var(--txt);"><i class="bi bi-upload me-2" style="color:#f87171;"></i>Upload Backup</h6>
                <button type="button" class="btn-close btn-close-white" data-bs-dismiss="modal"></button>
            </div>
            <div class="modal-body" style="display:flex;flex-direction:column;gap:1rem;">
                <div style="background:rgba(248,113,113,.1);border:1px solid rgba(248,113,113,.3);border-radius:8px;padding:.75rem 1rem;font-size:.82rem;color:#fca5a5;display:flex;gap:.6rem;align-items:flex-start;">
                    <i class="bi bi-exclamation-triangle-fill" style="flex-shrink:0;margin-top:.1rem;"></i>
                    <span><strong>Warning:</strong> This will <strong>delete all existing files</strong> in the server volume and replace them with the contents of the uploaded backup. This cannot be undone.</span>
                </div>
                <div>
                    <label class="form-label" style="font-size:.78rem;font-weight:600;color:var(--muted);text-transform:uppercase;letter-spacing:.06em;">Backup File (.tar.gz)</label>
                    <input type="file" id="b-restore-file" accept=".tar.gz,application/gzip" style="display:none;" onchange="onRestoreFileChosen()">
                    <div id="b-restore-dropzone" onclick="document.getElementById('b-restore-file').click()"
                         style="border:2px dashed rgba(255,255,255,.15);border-radius:8px;padding:1.25rem;text-align:center;cursor:pointer;font-size:.82rem;color:var(--muted);transition:border-color .2s;"
                         onmouseover="this.style.borderColor='rgba(248,113,113,.4)'" onmouseout="this.style.borderColor='rgba(255,255,255,.15)'">
                        <i class="bi bi-file-zip" style="font-size:1.4rem;display:block;margin-bottom:.4rem;color:#f87171;"></i>
                        Click to select a <code>.tar.gz</code> file
                    </div>
                    <div id="b-restore-filename" style="display:none;margin-top:.4rem;font-size:.8rem;color:#86efac;"><i class="bi bi-check-circle" style="margin-right:.3rem;"></i><span></span></div>
                </div>
                <div id="b-restore-progress" style="display:none;">
                    <div style="font-size:.78rem;color:var(--muted);margin-bottom:.3rem;">Uploading & restoring…</div>
                    <div style="background:rgba(255,255,255,.07);border-radius:4px;overflow:hidden;height:6px;">
                        <div id="b-restore-bar" style="height:100%;background:#f87171;width:0%;transition:width .3s;"></div>
                    </div>
                </div>
            </div>
            <div class="modal-footer d-flex gap-2 justify-content-end" style="border-top:1px solid rgba(255,255,255,.07);">
                <button type="button" class="btn-yu btn-yu-ghost" style="width:auto;" data-bs-dismiss="modal">Cancel</button>
                <button type="button" id="b-restore-btn" class="btn-yu btn-yu-primary" style="width:auto;background:#ef4444;border-color:#ef4444;" onclick="doRestore()" disabled>
                    <i class="bi bi-upload"></i> Restore
                </button>
            </div>
        </div>
    </div>
</div>`;

function _ensureModal() {
    let el = document.getElementById('createBackupModal');
    if (!el) {
        document.body.insertAdjacentHTML('beforeend', _MODAL_HTML);
        el = document.getElementById('createBackupModal');
    }
    return el;
}

function _ensureRestoreModal() {
    let el = document.getElementById('restoreBackupModal');
    if (!el) {
        document.body.insertAdjacentHTML('beforeend', _RESTORE_MODAL_HTML);
        el = document.getElementById('restoreBackupModal');
    }
    return el;
}

function _backupsOnPageShown(path) {
    if (!path.includes('/backups')) return;
    if (_bModal) { try { _bModal.dispose(); } catch {} _bModal = null; }
    if (_restoreModal) { try { _restoreModal.dispose(); } catch {} _restoreModal = null; }
    _bModal = new bootstrap.Modal(_ensureModal());
    _restoreModal = new bootstrap.Modal(_ensureRestoreModal());
    loadBackups();
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => _backupsOnPageShown(window.location.pathname), { once: true });
} else {
    _backupsOnPageShown(window.location.pathname);
}

window.addEventListener('yu:page-shown', (ev) => {
    _backupsOnPageShown(String(ev?.detail?.path || window.location.pathname));
});

// ── Data ───────────────────────────────────────────────────────────────────────

async function loadBackups() {
    try {
        const r = await fetch(`/api/servers/${BID}/backups`);
        const d = await r.json();
        if (!r.ok) { renderError(d.error || 'Failed to load'); return; }
        const maxBackups = d.max_backups ?? window.YU_MAX_BACKUPS ?? 10;
        renderBackups(d.backups || [], maxBackups);
        updateCreateBtn(d.backups?.length ?? 0, maxBackups);
    } catch {
        renderError('Network error');
    }
}

function updateCreateBtn(count, max) {
    const btn = document.getElementById('b-open-btn');
    if (!btn) return;
    const atLimit = count >= max;
    btn.disabled = atLimit || _backingUp;
    btn.title = atLimit ? `Backup limit reached (${count}/${max})` : '';
}

function renderError(msg) {
    document.getElementById('backup-list').innerHTML =
        `<div class="yu-panel" style="padding:2rem;text-align:center;color:var(--err);font-size:.85rem;">${escHtml(msg)}</div>`;
}

function fmtBytes(b) {
    if (b >= 1073741824) return (b / 1073741824).toFixed(2) + ' GB';
    if (b >= 1048576)    return (b / 1048576).toFixed(1) + ' MB';
    return (b / 1024).toFixed(0) + ' KB';
}

function fmtTime(ts) {
    if (!ts || ts === '—') return '—';
    try {
        const d = new Date(ts.replace(' ', 'T') + 'Z');
        return d.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
    } catch { return ts; }
}

function renderBackups(list, maxBackups) {
    const el = document.getElementById('backup-list');
    // Update counter badge
    const counter = document.getElementById('b-counter');
    if (counter) {
        const atLimit = list.length >= maxBackups;
        counter.textContent = `${list.length} / ${maxBackups}`;
        counter.style.color = atLimit ? '#f87171' : 'var(--muted)';
    }

    const loadingBanner = _backingUp ? `
    <div class="yu-panel" style="margin-bottom:.5rem;border-color:rgba(167,139,250,.3);background:rgba(167,139,250,.06);">
        <div style="padding:.65rem 1rem;display:flex;align-items:center;gap:.65rem;font-size:.82rem;color:#c4b5fd;">
            <span class="spinner-border spinner-border-sm" style="width:.85rem;height:.85rem;border-width:2px;flex-shrink:0;"></span>
            Backup in progress — downloads are disabled until it completes…
        </div>
    </div>` : '';

    if (!list.length) {
        el.innerHTML = loadingBanner + `
        <div class="yu-panel" style="padding:2.5rem;text-align:center;">
            <i class="bi bi-archive" style="font-size:2rem;color:var(--muted);opacity:.4;"></i>
            <p style="margin:.75rem 0 .35rem;font-size:.875rem;color:var(--muted);">No backups found.</p>
            <p style="font-size:.78rem;color:var(--muted);opacity:.7;margin:0;">Create one with the button above, or set up a Backup schedule.</p>
        </div>`;
        return;
    }
    el.innerHTML = loadingBanner + list.map(b => `
    <div class="yu-panel" style="margin-bottom:.5rem;">
        <div style="padding:.7rem 1rem;display:flex;align-items:center;gap:.75rem;flex-wrap:wrap;">
            <i class="bi bi-file-zip" style="font-size:1.1rem;color:#10b981;flex-shrink:0;"></i>
            <div style="flex:1;min-width:0;">
                <div style="font-size:.865rem;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;" title="${escHtml(b.name)}">${escHtml(b.name)}</div>
                <div style="font-size:.75rem;color:var(--muted);display:flex;gap:.85rem;flex-wrap:wrap;margin-top:.15rem;">
                    <span><i class="bi bi-hdd" style="margin-right:.25rem;"></i>${fmtBytes(b.size_bytes)}</span>
                    <span><i class="bi bi-calendar3" style="margin-right:.25rem;"></i>${fmtTime(b.created_at)}</span>
                </div>
            </div>
            <div style="display:flex;gap:.4rem;flex-shrink:0;">
                ${window.YU_IS_ADMIN ? `<a href="/api/servers/${BID}/backups/${encodeURIComponent(b.name)}/download"
                   class="btn-yu btn-yu-ghost${_backingUp ? ' disabled' : ''}" style="padding:.3rem .55rem;font-size:.75rem;text-decoration:none;${_backingUp ? 'pointer-events:none;opacity:.4;' : ''}"
                   title="${_backingUp ? 'Backup in progress…' : 'Download'}" download="${escHtml(b.name)}">
                    <i class="bi bi-download"></i>
                </a>` : ''}
                <button class="btn-yu btn-yu-ghost" style="padding:.3rem .55rem;font-size:.75rem;color:var(--err);"
                        onclick="deleteBackup(${escHtml(JSON.stringify(b.name))})" title="Delete">
                    <i class="bi bi-trash3"></i>
                </button>
            </div>
        </div>
    </div>`).join('');
}

// ── Modal ──────────────────────────────────────────────────────────────────────

function openCreateModal() {
    const el = document.getElementById('b-prefix');
    if (el) el.value = 'backup';
    _bModal.show();
}

function openRestoreModal() {
    // Reset state
    const fi = document.getElementById('b-restore-file');
    if (fi) fi.value = '';
    const fn_ = document.getElementById('b-restore-filename');
    if (fn_) { fn_.style.display = 'none'; fn_.querySelector('span').textContent = ''; }
    const prog = document.getElementById('b-restore-progress');
    if (prog) prog.style.display = 'none';
    const bar = document.getElementById('b-restore-bar');
    if (bar) bar.style.width = '0%';
    const btn = document.getElementById('b-restore-btn');
    if (btn) { btn.disabled = true; btn.innerHTML = '<i class="bi bi-upload"></i> Restore'; }
    _restoreModal.show();
}

function onRestoreFileChosen() {
    const fi = document.getElementById('b-restore-file');
    const fn_ = document.getElementById('b-restore-filename');
    const btn = document.getElementById('b-restore-btn');
    const file = fi?.files?.[0];
    if (!file) return;
    fn_.querySelector('span').textContent = file.name;
    fn_.style.display = 'block';
    if (btn) btn.disabled = false;
}

async function doRestore() {
    const fi = document.getElementById('b-restore-file');
    const file = fi?.files?.[0];
    if (!file) return;

    const btn = document.getElementById('b-restore-btn');
    const prog = document.getElementById('b-restore-progress');
    const bar = document.getElementById('b-restore-bar');
    if (btn) { btn.disabled = true; btn.innerHTML = '<i class="bi bi-hourglass-split"></i> Restoring…'; }
    if (prog) prog.style.display = 'block';

    const fd = new FormData();
    fd.append('file', file, file.name);

    try {
        await new Promise((resolve, reject) => {
            const xhr = new XMLHttpRequest();
            xhr.open('POST', `/api/servers/${BID}/backups/restore`);
            xhr.upload.onprogress = e => {
                if (e.lengthComputable && bar) {
                    bar.style.width = Math.round(e.loaded / e.total * 90) + '%';
                }
            };
            xhr.onload = () => {
                if (bar) bar.style.width = '100%';
                if (xhr.status >= 200 && xhr.status < 300) {
                    resolve();
                } else {
                    try { reject(JSON.parse(xhr.responseText).error || xhr.statusText); }
                    catch { reject(xhr.statusText); }
                }
            };
            xhr.onerror = () => reject('Network error');
            xhr.send(fd);
        });
        _restoreModal.hide();
        showToast('success', 'Backup restored successfully');
        loadBackups();
    } catch (err) {
        showToast('danger', String(err));
        if (btn) { btn.disabled = false; btn.innerHTML = '<i class="bi bi-upload"></i> Restore'; }
        if (prog) prog.style.display = 'none';
    }
}

async function createBackup() {
    const prefix = (document.getElementById('b-prefix')?.value || '').trim() || 'backup';
    const btn = document.getElementById('b-create-btn');
    const orig = btn?.innerHTML;
    if (btn) { btn.disabled = true; btn.innerHTML = '<i class="bi bi-hourglass-split"></i> Running…'; }

    _backingUp = true;
    // Disable open button and re-render to lock downloads
    const openBtn = document.getElementById('b-open-btn');
    if (openBtn) openBtn.disabled = true;
    // Re-fetch list to apply _backingUp state to download links
    loadBackups();

    try {
        const r = await fetch(`/api/servers/${BID}/backups/create`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ prefix }),
        });
        const d = await r.json();
        if (r.ok) {
            _bModal.hide();
            showToast('success', d.message || 'Backup created');
        } else {
            showToast('danger', d.error || 'Failed to create backup');
        }
    } catch {
        showToast('danger', 'Network error');
    }

    _backingUp = false;
    if (btn) { btn.disabled = false; btn.innerHTML = orig; }
    loadBackups();
}

async function deleteBackup(name) {
    const ok = await window.yuConfirm(`Delete backup "${name}"?`, {
        icon: 'bi-trash3-fill',
        iconColor: '#f87171',
        subtitle: 'This cannot be undone.',
        okLabel: 'Delete',
    });
    if (!ok) return;
    try {
        const r = await fetch(`/api/servers/${BID}/backups/${encodeURIComponent(name)}/delete`, { method: 'POST' });
        const d = await r.json();
        if (r.ok) { showToast('success', 'Backup deleted'); loadBackups(); }
        else showToast('danger', d.error || 'Failed to delete');
    } catch { showToast('danger', 'Network error'); }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

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
    const bg = {success:'rgba(34,197,94,.14)', danger:'rgba(239,68,68,.14)', warning:'rgba(251,191,36,.14)'};
    const bd = {success:'rgba(34,197,94,.3)',  danger:'rgba(239,68,68,.3)',  warning:'rgba(251,191,36,.3)'};
    const tx = {success:'#86efac',            danger:'#fca5a5',             warning:'#fde68a'};
    const el = document.createElement('div');
    el.style.cssText = `background:${bg[type]||bg.danger};border:1px solid ${bd[type]||bd.danger};color:${tx[type]||tx.danger};padding:.55rem 1rem;border-radius:8px;font-size:.825rem;font-weight:500;backdrop-filter:blur(8px);opacity:0;transform:translateX(20px);transition:all .25s;white-space:nowrap;`;
    el.textContent = msg;
    tc.appendChild(el);
    requestAnimationFrame(() => requestAnimationFrame(() => { el.style.opacity = '1'; el.style.transform = 'none'; }));
    setTimeout(() => { el.style.opacity = '0'; el.style.transform = 'translateX(20px)'; setTimeout(() => el.remove(), 280); }, 3400);
}
