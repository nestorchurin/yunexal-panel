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
    document.body.dispatchEvent(new CustomEvent('file-created'));
}

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

    // Paste
    ctxEl('fb-ctx-paste')?.addEventListener('click', async function () {
        if (!_clipboard) return;
        ctxHide();
        const fd = new URLSearchParams();
        fd.append('src',     _clipboard.path);
        fd.append('dst_dir', currentBrowserPath());
        try {
            const res = await fetch(`/api/servers/${getServerId()}/files/copy`, {
                method: 'POST',
                body: fd,
                headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            });
            if (res.ok) { showToast('Pasted', 'ok'); refreshBrowser(); }
            else showToast(await res.text() || 'Paste failed', 'err');
        } catch (err) { showToast(err.message, 'err'); }
    });

    // Delete
    ctxEl('fb-ctx-delete')?.addEventListener('click', async function () {
        if (!_ctxTarget) return;
        const name = _ctxTarget.dataset.path.split('/').pop();
        ctxHide();
        if (!confirm('Delete "' + name + '"?\nThis cannot be undone.')) return;
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
}

// Script is at bottom of <body> so DOM is already parsed — call immediately
fbInit();

// ── File upload ───────────────────────────────────────────────────────────────

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
