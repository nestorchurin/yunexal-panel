// File Browser JS  — rewritten

// ── Utilities ─────────────────────────────────────────────────────────────────

function showToast(msg, type) {
    const tc = document.getElementById('toastContainer');
    if (!tc) return;
    const el = document.createElement('div');
    el.className = `toast-msg toast-${type}`;
    el.textContent = msg;
    tc.appendChild(el);
    requestAnimationFrame(() => requestAnimationFrame(() => el.classList.add('show')));
    setTimeout(() => { el.classList.remove('show'); setTimeout(() => el.remove(), 300); }, 3500);
}

function getServerId() {
    const m = location.pathname.match(/\/servers\/(\d+)/);
    return m ? m[1] : null;
}

function currentBrowserPath() {
    return document.getElementById('file-browser')?.dataset?.path || '/';
}

// Dispatch on body so HTMX "hx-trigger='file-created from:body'" picks it up
function refreshBrowser() {
    fbClearSelection();
    const fb = document.getElementById('file-browser');
    if (!fb) return;
    const path = fb.dataset.path || '/';
    const sid  = getServerId();
    htmx.ajax('GET', `/api/servers/${sid}/files/list?path=${encodeURIComponent(path)}`, {
        target: '#file-browser',
        swap: 'outerHTML',
    });
}

// ── Custom confirm dialog ──────────────────────────────────────────────────────
// Returns a Promise<boolean> — resolves true on confirm, false on cancel.
// ── "New File" modal ──────────────────────────────────────────────────────────

function fbOpenCreate() {
    const path = currentBrowserPath();
    const inp = document.getElementById('fb-create-path');
    if (inp) inp.value = path;
    const lbl = document.getElementById('fb-create-dir');
    if (lbl) lbl.textContent = path === '/' ? '/ (root)' : path;
    // clear name field
    const name = document.getElementById('fb-create-name');
    if (name) name.value = '';
    new bootstrap.Modal(document.getElementById('createFileModal')).show();
}

// After HTMX file creation request
document.body.addEventListener('htmx:afterRequest', function (e) {
    const el = e.detail.elt;
    if (!el || typeof el.getAttribute !== 'function') return;
    const action = el.getAttribute('hx-post') || '';
    if (!action.includes('files/create')) return;

    if (e.detail.successful) {
        showToast('File created', 'ok');
        bootstrap.Modal.getInstance(document.getElementById('createFileModal'))?.hide();
    } else {
        showToast(e.detail.xhr?.responseText || 'Failed to create file', 'err');
    }
});

// Keyboard shortcut: n = new file
document.addEventListener('keydown', function (e) {
    if (document.activeElement && ['INPUT', 'TEXTAREA', 'SELECT'].includes(document.activeElement.tagName)) return;
    if (e.key === 'n' && !e.ctrlKey && !e.metaKey) fbOpenCreate();
});

// ── Context Menu ──────────────────────────────────────────────────────────────

let _ctxTarget = null;   // the .fb-row element right-clicked
let _clipboard  = null;  // { path, type }
const _selection = new Set(); // full paths currently checked for multi-select

function ctxEl(id) { return document.getElementById(id); }
function ctxHide()  { ctxEl('fb-ctx-menu')?.classList.remove('open'); }

function ctxShow(x, y, row) {
    const menu = ctxEl('fb-ctx-menu');
    if (!menu) return;
    _ctxTarget = row;
    const type    = row.dataset.type; // "file" | "dir"
    const hasClip = !!_clipboard;

    const editEl  = ctxEl('fb-ctx-edit');
    const openEl  = ctxEl('fb-ctx-open');
    const pasteEl = ctxEl('fb-ctx-paste');
    if (editEl)  editEl.style.display  = type === 'file' ? '' : 'none';
    if (openEl)  openEl.style.display  = type === 'dir'  ? '' : 'none';
    if (pasteEl) pasteEl.classList.toggle('disabled', !hasClip);

    const extractEl  = ctxEl('fb-ctx-extract');
    const extractSep = ctxEl('fb-ctx-sep-archive');
    const isArchive  = row.dataset.archive === 'true';
    if (extractEl)  extractEl.style.display  = isArchive ? '' : 'none';
    if (extractSep) extractSep.style.display = isArchive ? '' : 'none';

    const hasSel     = _selection.size > 0;
    const archSelEl  = ctxEl('fb-ctx-archive-sel');
    const archSelSep = ctxEl('fb-ctx-sep-sel');
    if (archSelEl) {
        archSelEl.style.display = hasSel ? '' : 'none';
        archSelEl.innerHTML = `<i class="bi bi-file-earmark-zip-fill"></i> Archive selected (${_selection.size})…`;
    }
    if (archSelSep) archSelSep.style.display = hasSel ? '' : 'none';

    // Reposition — keep inside viewport
    menu.style.left = '-9999px';
    menu.style.top  = '-9999px';
    menu.classList.add('open');
    const mw = menu.offsetWidth, mh = menu.offsetHeight;
    menu.style.left = (x + mw > innerWidth  ? x - mw : x) + 'px';
    menu.style.top  = (y + mh > innerHeight ? y - mh : y) + 'px';
}

// Close on outside click or Escape
document.addEventListener('click',   () => ctxHide());
document.addEventListener('keydown', e => { if (e.key === 'Escape') ctxHide(); });

// Right-click on any .fb-row (delegation — HTMX replaces the list)
document.addEventListener('contextmenu', function (e) {
    const row = e.target.closest('.fb-row[data-path]');
    if (!row) { ctxHide(); return; }
    e.preventDefault();
    ctxShow(e.clientX, e.clientY, row);
});

// ── Wire up actions + drag-drop after DOM is ready ────────────────────────────
// (Script is at bottom of <body>, so DOM is already ready — call immediately)

function fbInit() {

    // Edit (files only)
    ctxEl('fb-ctx-edit')?.addEventListener('click', function () {
        if (!_ctxTarget) return;
        location.href = `/servers/${getServerId()}/files/edit?path=${encodeURIComponent(_ctxTarget.dataset.path)}`;
    });

    // Open (dirs only) — use htmx.ajax for a proper HTMX swap
    ctxEl('fb-ctx-open')?.addEventListener('click', function () {
        if (!_ctxTarget) return;
        ctxHide();
        htmx.ajax('GET',
            `/api/servers/${getServerId()}/files/list?path=${encodeURIComponent(_ctxTarget.dataset.path)}`,
            { target: '#file-browser', swap: 'outerHTML' }
        );
    });

    // Rename — open modal
    ctxEl('fb-ctx-rename')?.addEventListener('click', function () {
        if (!_ctxTarget) return;
        const parts   = _ctxTarget.dataset.path.split('/');
        const curName = parts[parts.length - 1] || '';
        const pathInp = ctxEl('fb-rename-path');
        const nameInp = ctxEl('fb-rename-name');
        if (pathInp) pathInp.value = _ctxTarget.dataset.path;
        if (nameInp) nameInp.value = curName;
        ctxHide();
        const modal = ctxEl('renameModal');
        if (modal) {
            new bootstrap.Modal(modal).show();
            setTimeout(() => nameInp?.select(), 300);
        }
    });

    // Copy
    ctxEl('fb-ctx-copy')?.addEventListener('click', function () {
        if (!_ctxTarget) return;
        _clipboard = { path: _ctxTarget.dataset.path, type: _ctxTarget.dataset.type };
        showToast('Copied: ' + _ctxTarget.dataset.path.split('/').pop(), 'ok');
        ctxHide();
    });

    // Paste — handles single { path, type } and multi { paths[], mode }
    ctxEl('fb-ctx-paste')?.addEventListener('click', async function () {
        if (!_clipboard) return;
        ctxHide();
        const sid      = getServerId();
        const dst      = currentBrowserPath();
        const endpoint = _clipboard.mode === 'move' ? 'move' : 'copy';
        const paths    = _clipboard.paths || [_clipboard.path];
        let ok = 0; const errs = [];
        for (const src of paths) {
            const fd = new URLSearchParams();
            fd.append('src',     src);
            fd.append('dst_dir', dst);
            try {
                const res = await fetch(`/api/servers/${sid}/files/${endpoint}`, {
                    method: 'POST', body: fd,
                    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                });
                if (res.ok) ok++;
                else errs.push(await res.text() || src.split('/').pop());
            } catch (e) { errs.push(e.message); }
        }
        if (errs.length) showToast(errs[0] || 'Paste failed', 'err');
        else { showToast(`Pasted ${ok} item(s)`, 'ok'); refreshBrowser(); }
        if (ok > 0 && _clipboard.mode === 'move') _clipboard = null;
    });

    // Delete
    ctxEl('fb-ctx-delete')?.addEventListener('click', async function () {
        if (!_ctxTarget) return;
        const name = _ctxTarget.dataset.path.split('/').pop();
        ctxHide();
        if (!await yuConfirm(`Delete "${name}"?`)) return;
        try {
            const res = await fetch(
                `/api/servers/${getServerId()}/files/delete?path=${encodeURIComponent(_ctxTarget.dataset.path)}`,
                { method: 'POST' }
            );
            if (res.ok) { showToast('Deleted: ' + name, 'ok'); refreshBrowser(); }
            else showToast(await res.text() || 'Delete failed', 'err');
        } catch (err) { showToast(err.message, 'err'); }
    });

    // ── Rename modal form submit ──────────────────────────────────────────────

    ctxEl('fb-rename-form')?.addEventListener('submit', async function (e) {
        e.preventDefault();
        const path    = ctxEl('fb-rename-path')?.value || '';
        const newName = (ctxEl('fb-rename-name')?.value || '').trim();
        if (!newName) return;
        const modal = ctxEl('renameModal');
        if (modal) bootstrap.Modal.getInstance(modal)?.hide();
        const fd = new URLSearchParams();
        fd.append('path',     path);
        fd.append('new_name', newName);
        try {
            const res = await fetch(`/api/servers/${getServerId()}/files/rename`, {
                method: 'POST',
                body: fd,
                headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            });
            if (res.ok) { showToast('Renamed', 'ok'); refreshBrowser(); }
            else showToast(await res.text() || 'Rename failed', 'err');
        } catch (err) { showToast(err.message, 'err'); }
    });

    // ── Drag-and-drop upload ──────────────────────────────────────────────────
    // IMPORTANT: listen on `document` so that dragover is prevented even when
    // the cursor is over a child element (<a class="fb-row">, etc.).
    // A listener only on #fb-drop-zone would NOT prevent the browser's "no drop"
    // cursor when hovering over children that have no dragover handler.

    const dropZone = ctxEl('fb-drop-zone');
    if (!dropZone) return;

    document.addEventListener('dragover', function (e) {
        if (!e.dataTransfer?.types || !Array.from(e.dataTransfer.types).includes('Files')) return;
        e.preventDefault();  // must prevent on EVERY dragover, even outside dropZone,
        e.dataTransfer.dropEffect = 'copy'; // or browser locks the cursor to "no-drop"
        if (dropZone.contains(e.target)) {
            dropZone.classList.add('drag-over');
        } else {
            dropZone.classList.remove('drag-over');
        }
    });

    document.addEventListener('dragleave', function (e) {
        // Only remove the class when the cursor truly leaves the drop zone
        // (relatedTarget is null or outside the zone)
        if (!dropZone.classList.contains('drag-over')) return;
        if (!e.relatedTarget || !dropZone.contains(e.relatedTarget)) {
            dropZone.classList.remove('drag-over');
        }
    });

    document.addEventListener('drop', async function (e) {
        if (!dropZone.contains(e.target)) return;
        e.preventDefault();
        dropZone.classList.remove('drag-over');
        const files = e.dataTransfer?.files;
        if (files && files.length > 0) await fbUploadFiles(files);
    });

    // ── Multi-select checkboxes ─────────────────────────────────────────────
    document.addEventListener('change', function (e) {
        if (e.target.classList.contains('fb-cb') && e.target.id !== 'fb-cb-all') {
            const path = e.target.value;
            if (e.target.checked) _selection.add(path); else _selection.delete(path);
            e.target.closest('.fb-row')?.classList.toggle('fb-selected', e.target.checked);
            const allCb = document.getElementById('fb-cb-all');
            if (allCb) {
                const cbs = [...document.querySelectorAll('.fb-cb:not(#fb-cb-all)')];
                allCb.indeterminate = cbs.some(c => c.checked) && !cbs.every(c => c.checked);
                allCb.checked      = cbs.length > 0 && cbs.every(c => c.checked);
            }
            fbUpdateSelBar();
        }
        if (e.target.id === 'fb-cb-all') {
            const checked = e.target.checked;
            document.querySelectorAll('.fb-cb:not(#fb-cb-all)').forEach(cb => {
                cb.checked = checked;
                cb.closest('.fb-row')?.classList.toggle('fb-selected', checked);
                if (checked) _selection.add(cb.value); else _selection.delete(cb.value);
            });
            fbUpdateSelBar();
        }
    });

    // Extract archive contextmenu item
    ctxEl('fb-ctx-extract')?.addEventListener('click', function () {
        if (!_ctxTarget) return;
        ctxHide();
        fbExtractArchive(_ctxTarget.dataset.path);
    });

    // Archive selected items (context menu)
    ctxEl('fb-ctx-archive-sel')?.addEventListener('click', function () {
        ctxHide();
        fbArchiveSelected();
    });

    // Selection bar buttons live inside HTMX-rendered content — delegate
    document.addEventListener('click', function (e) {
        if (e.target.closest('#fb-btn-archive')) fbArchiveSelected();
        if (e.target.closest('#fb-btn-copy'))    fbCopySelected('copy');
        if (e.target.closest('#fb-btn-cut'))     fbCopySelected('move');
        if (e.target.closest('#fb-btn-delete'))  fbBulkDelete();
    });

    // Clear selection state whenever HTMX re-renders the file browser
    document.body.addEventListener('htmx:afterSwap', fbClearSelection);
}

// Script is at bottom of <body> so DOM is already parsed — call immediately
fbInit();

// ── Eager initial load (fixes SPA navigation delay) ───────────────────────────
// hx-trigger="load" only fires after htmx.process() which runs *after* all scripts.
// We trigger the first listing immediately from JS instead.
(function fbEagerLoad() {
    const fb = document.getElementById('file-browser');
    if (!fb || fb.dataset.path !== '/') return; // already loaded (non-root path means htmx already ran)
    const sid = getServerId();
    if (!sid || !window.htmx) return;
    htmx.ajax('GET', `/api/servers/${sid}/files/list?path=/`, {
        target: '#file-browser',
        swap: 'outerHTML',
    });
})();

// ── Auto-refresh: JSON diff (dashboard-style, no flicker) ────────────────────

// Builds a single .fb-row element from a JSON entry (used when inserting new rows).
function fbBuildRow(e, sid) {
    const el = document.createElement('a');
    el.dataset.path = e.path;

    if (e.is_dir) {
        el.className = 'fb-row fb-row-dir';
        el.dataset.type = 'dir';
        el.setAttribute('hx-get', `/api/servers/${sid}/files/list?path=${encodeURIComponent(e.path)}`);
        el.setAttribute('hx-target', '#file-browser');
        el.setAttribute('hx-swap', 'outerHTML');
    } else {
        el.className = 'fb-row fb-row-file';
        el.dataset.type = 'file';
        if (e.is_archive) el.dataset.archive = 'true';
        el.href = `/servers/${sid}/files/edit?path=${encodeURIComponent(e.path)}`;
    }

    // Checkbox
    const cbWrap = document.createElement('label');
    cbWrap.className = 'fb-cb-wrap';
    cbWrap.addEventListener('click', ev => ev.stopPropagation());
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.className = 'fb-cb';
    cb.value = e.path;
    const cbBox = document.createElement('span');
    cbBox.className = 'fb-cb-box';
    cbWrap.append(cb, cbBox);

    // Icon
    const iconDiv = document.createElement('div');
    iconDiv.className = `fb-icon ${e.is_dir ? 'fb-icon-dir' : e.color}`;
    const iconEl = document.createElement('i');
    iconEl.className = `bi ${e.icon}`;
    iconDiv.appendChild(iconEl);

    // Name (textContent — safe against XSS)
    const nameDiv = document.createElement('div');
    nameDiv.className = 'fb-name';
    nameDiv.textContent = e.name;

    // Meta
    const metaDiv = document.createElement('div');
    metaDiv.className = 'fb-meta';
    metaDiv.textContent = e.meta;

    el.append(cbWrap, iconDiv, nameDiv, metaDiv);
    return el;
}

// Poll the JSON endpoint and diff the existing .fb-row list in-place.
async function fbPollJson() {
    if (document.hidden) return;
    const progress = document.getElementById('fb-upload-progress');
    if (progress && progress.style.display !== 'none' && progress.style.display !== '') return;

    const fb = document.getElementById('file-browser');
    if (!fb) return;
    const path = fb.dataset.path || '/';
    const sid  = getServerId();
    if (!sid) return;

    let entries;
    try {
        const res = await fetch(`/api/servers/${sid}/files/list-json?path=${encodeURIComponent(path)}`);
        if (!res.ok) return;
        entries = await res.json();
    } catch (_) { return; }

    const list = fb.querySelector('.fb-list');
    if (!list) return;

    // Build lookup: path → entry
    const entryMap = new Map(entries.map(e => [e.path, e]));

    // Update existing rows or remove deleted ones
    for (const row of [...list.querySelectorAll('.fb-row[data-path]')]) {
        if (row.classList.contains('fb-row-back')) continue;
        const p = row.dataset.path;
        if (!entryMap.has(p)) {
            _selection.delete(p);
            row.remove();
        } else {
            const metaEl = row.querySelector('.fb-meta');
            const newMeta = entryMap.get(p).meta;
            if (metaEl && metaEl.textContent !== newMeta) metaEl.textContent = newMeta;
        }
    }

    // Insert rows for newly-appeared files
    const existingPaths = new Set(
        [...list.querySelectorAll('.fb-row[data-path]')].map(r => r.dataset.path)
    );
    for (const e of entries) {
        if (existingPaths.has(e.path)) continue;
        const row = fbBuildRow(e, sid);
        list.appendChild(row);
        if (window.htmx) htmx.process(row);
    }

    // Sync empty-state placeholder
    const hasRows = list.querySelector('.fb-row:not(.fb-row-back)');
    const emptyEl = list.querySelector('.fb-empty');
    if (!hasRows && !emptyEl) {
        const empty = document.createElement('div');
        empty.className = 'fb-empty';
        empty.innerHTML = '<i class="bi bi-folder2-open"></i><div>This folder is empty</div>';
        list.appendChild(empty);
    } else if (hasRows && emptyEl) {
        emptyEl.remove();
    }

    fbUpdateSelBar();
}

const _fbRefreshTimer = setInterval(fbPollJson, 5000);

// ── Cleanup (SPA navigation teardown) ────────────────────────────────────────
window._yuPageCleanup = function () {
    clearInterval(_fbRefreshTimer);
    window._yuPageCleanup = undefined;
};

// ── Selection bar ──────────────────────────────────────────────────────────────────────

function fbUpdateSelBar() {
    const bar = document.getElementById('fb-sel-bar');
    const cnt = document.getElementById('fb-sel-count');
    const n   = _selection.size;
    if (cnt) cnt.textContent = n ? `${n} selected` : '';
    if (bar) bar.classList.toggle('visible', n > 0);
    // Persist checkboxes visibility on all rows while anything is selected
    const browser = document.getElementById('file-browser');
    if (browser) browser.classList.toggle('sel-mode', n > 0);
}

function fbClearSelection() {
    _selection.clear();
    fbUpdateSelBar();
}

// ── Copy / Cut selected items to clipboard ────────────────────────────────────────────

function fbCopySelected(mode) {
    if (_selection.size === 0) return;
    _clipboard = { paths: [..._selection], mode };
    const word = mode === 'move' ? 'Cut' : 'Copied';
    showToast(`${word} ${_selection.size} item(s) — navigate to destination and Paste`, 'ok');
    if (mode === 'move') fbClearSelection();
}

// ── Bulk delete selected items ──────────────────────────────────────────────────────────

async function fbBulkDelete() {
    if (_selection.size === 0) return;
    if (!await yuConfirm(`Delete ${_selection.size} selected item(s)?`)) return;
    const fd = new URLSearchParams();
    fd.append('paths', [..._selection].join('\n'));
    try {
        const res = await fetch(`/api/servers/${getServerId()}/files/bulk-delete`, {
            method: 'POST', body: fd,
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            credentials: 'same-origin',
        });
        if (res.ok) {
            showToast(`Deleted ${_selection.size} item(s)`, 'ok');
            refreshBrowser();
        } else {
            showToast(await res.text() || 'Delete failed', 'err');
        }
    } catch (err) { showToast(err.message, 'err'); }
}

// ── Archive selected items as tar.gz ───────────────────────────────────────────────────

async function fbArchiveSelected() {
    if (_selection.size === 0) return;
    const name = prompt('Archive name (without extension):', 'archive');
    if (!name || !name.trim()) return;
    const sid = getServerId();
    const fd  = new URLSearchParams();
    fd.append('dir',   currentBrowserPath());
    fd.append('name',  name.trim());
    fd.append('paths', [..._selection].join('\n'));
    try {
        const res = await fetch(`/api/servers/${sid}/files/archive`, {
            method: 'POST', body: fd,
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            credentials: 'same-origin',
        });
        if (res.ok) {
            showToast(`Archived ${_selection.size} item(s) \u2192 ${name.trim()}.tar.gz`, 'ok');
            refreshBrowser();
        } else {
            showToast(await res.text() || 'Archive failed', 'err');
        }
    } catch (err) { showToast(err.message, 'err'); }
}

// ── Extract archive in-place ────────────────────────────────────────────────────────────

async function fbExtractArchive(path) {
    const sid = getServerId();
    const fd  = new URLSearchParams();
    fd.append('path', path);
    try {
        const res = await fetch(`/api/servers/${sid}/files/extract`, {
            method: 'POST', body: fd,
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            credentials: 'same-origin',
        });
        if (res.ok) {
            showToast('Extracted: ' + path.split('/').pop(), 'ok');
            refreshBrowser();
        } else {
            showToast(await res.text() || 'Extract failed', 'err');
        }
    } catch (err) { showToast(err.message, 'err'); }
}

// ── File upload ─────────────────────────────────────────────────────────────────────

async function fbUploadFiles(files) {
    const sid   = getServerId();
    const path  = currentBrowserPath();
    const total = files.length;
    let ok = 0;

    const bar     = document.getElementById('fb-up-bar');
    const label   = document.getElementById('fb-up-label');
    const counter = document.getElementById('fb-up-counter');
    const panel   = document.getElementById('fb-upload-progress');

    panel.style.display = 'block';

    for (let i = 0; i < total; i++) {
        const file = files[i];
        counter.textContent = `${i + 1} / ${total}`;
        label.textContent   = file.name;
        bar.style.width     = '0%';

        const fd = new FormData();
        fd.append('file', file, file.name);
        const url = `/api/servers/${sid}/files/upload?path=${encodeURIComponent(path)}`;

        const result = await new Promise(resolve => {
            const xhr = new XMLHttpRequest();
            xhr.open('POST', url);

            xhr.upload.addEventListener('progress', e => {
                if (e.lengthComputable) {
                    bar.style.width = Math.round(e.loaded / e.total * 100) + '%';
                }
            });

            xhr.addEventListener('load', () => {
                bar.style.width = '100%';
                resolve({ ok: xhr.status >= 200 && xhr.status < 300, body: xhr.responseText });
            });

            xhr.addEventListener('error', () => resolve({ ok: false, body: 'network error' }));
            xhr.addEventListener('abort', () => resolve({ ok: false, body: 'aborted' }));

            xhr.send(fd);
        });

        if (result.ok) {
            ok++;
        } else {
            showToast('Failed: ' + file.name + ' — ' + result.body, 'err');
        }
    }

    panel.style.display = 'none';
    bar.style.width = '0%';

    if (ok > 0) {
        showToast('Uploaded ' + ok + ' file' + (ok > 1 ? 's' : ''), 'ok');
        refreshBrowser();
    }
}
